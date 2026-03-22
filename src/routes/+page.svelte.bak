<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { listen } from '@tauri-apps/api/event';

	import { audioEngine } from '$lib/audio/engine';
	import { projectStore } from '$lib/stores/projectStore';
	import { startSidecar, startHealthPolling, updateSidecarStatus, type SidecarId } from '$lib/stores/sidecarStore';
	import { invoke } from '@tauri-apps/api/core';
	import { save } from '@tauri-apps/plugin-dialog';
	import { activeTasks } from '$lib/stores/aiTaskStore';
	import { activePanelStore } from '$lib/stores/uiStore';

	import Transport from '$lib/components/Transport.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import StatusBar from '$lib/components/StatusBar.svelte';

	import Timeline from '$lib/components/Timeline.svelte';
	import StemSeparator from '$lib/components/StemSeparator.svelte';
	import EffectsRack from '$lib/components/EffectsRack.svelte';
	import PianoRoll from '$lib/components/PianoRoll.svelte';
	import Transcribe from '$lib/components/Transcribe.svelte';
	import MasterPanel from '$lib/components/MasterPanel.svelte';
	import PlaceholderPanel from '$lib/components/PlaceholderPanel.svelte';

	let healthPollInterval: ReturnType<typeof setInterval>;
	let unlisten: (() => void) | undefined;

	onMount(async () => {
		await audioEngine.init();
		healthPollInterval = startHealthPolling();
		await startSidecar('audio-fx');

		unlisten = await listen<{ id: string; status: string }>('sidecar-status', (event) => {
			const { id, status } = event.payload;
			updateSidecarStatus(
				id as SidecarId,
				status as 'stopped' | 'starting' | 'running' | 'degraded' | 'error'
			);
		});
	});

	onDestroy(() => {
		audioEngine.dispose();
		clearInterval(healthPollInterval);
		unlisten?.();
	});

	$: audioEngine.bpm = $projectStore.bpm;

	async function exportMix() {
		const outputPath = await save({
			filters: [{ name: 'WAV Audio', extensions: ['wav'] }],
			defaultPath: `${$projectStore.name}.wav`
		});
		if (!outputPath) return;

		// Render via OfflineAudioContext in the browser, then write to disk
		const ctx = new OfflineAudioContext(2, 44100 * 60, 44100);
		const clipLoads: Promise<void>[] = [];

		for (const track of $projectStore.tracks) {
			if (track.muted) continue;
			for (const clip of track.clips) {
				if (!clip.filePath) continue;
				clipLoads.push((async () => {
					const { readFile } = await import('@tauri-apps/plugin-fs');
					const bytes = await readFile(clip.filePath!);
					const decoded = await ctx.decodeAudioData(bytes.buffer);
					const src = ctx.createBufferSource();
					src.buffer = decoded;
					const gain = ctx.createGain();
					gain.gain.value = track.volume;
					src.connect(gain).connect(ctx.destination);
					src.start(0);
				})());
			}
		}

		await Promise.all(clipLoads);
		const rendered = await ctx.startRendering();

		// Convert AudioBuffer to WAV bytes
		const wav = audioBufferToWav(rendered);
		await invoke('export_wav', { outputPath, audioData: Array.from(new Uint8Array(wav)) });
		alert(`Exported to ${outputPath}`);
	}

	function audioBufferToWav(buffer: AudioBuffer): ArrayBuffer {
		const numChannels = buffer.numberOfChannels;
		const length = buffer.length * numChannels * 2;
		const view = new DataView(new ArrayBuffer(44 + length));
		// RIFF header
		const writeStr = (o: number, s: string) => { for (let i = 0; i < s.length; i++) view.setUint8(o + i, s.charCodeAt(i)); };
		writeStr(0, 'RIFF');
		view.setUint32(4, 36 + length, true);
		writeStr(8, 'WAVE');
		writeStr(12, 'fmt ');
		view.setUint32(16, 16, true);
		view.setUint16(20, 1, true);
		view.setUint16(22, numChannels, true);
		view.setUint32(24, buffer.sampleRate, true);
		view.setUint32(28, buffer.sampleRate * numChannels * 2, true);
		view.setUint16(32, numChannels * 2, true);
		view.setUint16(34, 16, true);
		writeStr(36, 'data');
		view.setUint32(40, length, true);
		let offset = 44;
		for (let i = 0; i < buffer.length; i++) {
			for (let ch = 0; ch < numChannels; ch++) {
				const s = Math.max(-1, Math.min(1, buffer.getChannelData(ch)[i]));
				view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7fff, true);
				offset += 2;
			}
		}
		return view.buffer;
	}
</script>

<svelte:head>
	<title>{$projectStore.name} — MusicStudio</title>
</svelte:head>

<div class="app-shell">
	<header class="app-header">
		<div class="app-logo">
			<span class="logo-icon">♫</span>
			<span class="logo-text">MusicStudio</span>
		</div>

		<nav class="menu-bar">
			<button on:click={() => invoke('save_project', { project: $projectStore })}>Save</button>
			<button on:click={exportMix} disabled={$projectStore.tracks.length === 0}>Export WAV</button>
		</nav>

		<Transport />

		<div class="header-right">
			{#if $activeTasks.length > 0}
				<span class="ai-badge">
					⚙ {$activeTasks[0].label}
					{#if $activeTasks[0].progress > 0}({$activeTasks[0].progress}%){/if}
				</span>
			{/if}
		</div>
	</header>

	<div class="workspace">
		<Sidebar />

		<main class="main-area">
			{#if $activePanelStore === 'timeline'}
				<Timeline />
			{:else if $activePanelStore === 'stem-sep'}
				<StemSeparator />
			{:else if $activePanelStore === 'fx-rack'}
				<EffectsRack />
			{:else if $activePanelStore === 'piano-roll'}
				<PianoRoll />
			{:else if $activePanelStore === 'transcribe'}
				<Transcribe />
			{:else if $activePanelStore === 'master'}
				<MasterPanel />
			{:else if $activePanelStore === 'voice'}
				<PlaceholderPanel title="Voice Studio"   icon="🎤" description="Voice cloning with RVC v3 and GPT-SoVITS — coming in Phase 3." />
			{:else if $activePanelStore === 'song-factory'}
				<PlaceholderPanel title="Song Factory"   icon="✨" description="Full-song generation via DiffRhythm and YuE — coming in Phase 4." />
			{:else if $activePanelStore === 'backing'}
				<PlaceholderPanel title="Backing Tracks" icon="🎵" description="Text-to-music via MusicGen — coming in Phase 4." />
			{:else if $activePanelStore === 'models'}
				<PlaceholderPanel title="Model Library"  icon="📦" description="Download and manage AI models — coming soon." />
			{:else}
				<Timeline />
			{/if}
		</main>
	</div>

	<StatusBar />
</div>

<style>
	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
	}

	.app-header {
		display: flex;
		align-items: center;
		gap: 12px;
		height: var(--header-height);
		background: var(--bg-surface);
		border-bottom: 1px solid var(--border);
		padding: 0 12px;
		flex-shrink: 0;
		user-select: none;
	}

	.app-logo {
		display: flex;
		align-items: center;
		gap: 6px;
		font-weight: 700;
		color: var(--accent-hover);
		flex-shrink: 0;
	}

	.logo-icon { font-size: 18px; }
	.logo-text { font-size: 14px; letter-spacing: -0.3px; }

	.menu-bar {
		display: flex;
		gap: 4px;
		flex-shrink: 0;
	}

	.menu-bar button {
		padding: 4px 10px;
		border-radius: 4px;
		color: var(--text-secondary);
		font-size: 12px;
		transition: background 0.1s, color 0.1s;
	}

	.menu-bar button:hover {
		background: var(--bg-highlight);
		color: var(--text-primary);
	}

	.header-right { margin-left: auto; }

	.ai-badge {
		background: var(--accent-dim);
		color: var(--accent-hover);
		border: 1px solid var(--accent);
		border-radius: 12px;
		padding: 2px 10px;
		font-size: 11px;
		animation: pulse 2s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50%       { opacity: 0.6; }
	}

	.workspace {
		display: flex;
		flex: 1;
		overflow: hidden;
	}

	.main-area {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}
</style>
