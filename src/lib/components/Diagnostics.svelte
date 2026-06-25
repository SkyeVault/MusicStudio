<script lang="ts">
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { sidecarStore, startSidecar, type SidecarId } from '$lib/stores/sidecarStore';

	interface ToolStatus { name: string; available: boolean; detail: string; }

	const TOOL_INFO: Record<string, { used_by: string; install_hint: string }> = {
		ffmpeg:  { used_by: 'Screen Recorder, Video thumbnails, duration probing', install_hint: 'sudo apt install ffmpeg' },
		ffprobe: { used_by: 'Video Timeline (duration probing)',                    install_hint: 'sudo apt install ffmpeg' },
		melt:    { used_by: 'Export Video',                                          install_hint: 'sudo apt install melt' },
		xrandr:  { used_by: 'Screen Recorder (resolution detection)',               install_hint: 'sudo apt install x11-xserver-utils' },
	};

	const SIDECAR_INFO: Record<SidecarId, { label: string; setup_hint: string }> = {
		'audio-fx': { label: 'Audio FX',  setup_hint: 'python3.11 -m venv sidecars/audio-fx/venv && pip install -r sidecars/audio-fx/requirements.txt' },
		'voice':    { label: 'Voice',     setup_hint: 'python3 -m venv sidecars/voice/venv && pip install -r sidecars/voice/requirements.txt' },
		'song-gen': { label: 'Song Gen',  setup_hint: 'python3 -m venv sidecars/song-gen/venv && pip install -r sidecars/song-gen/requirements.txt' },
		'stem-sep': { label: 'Stem Sep',  setup_hint: 'python3 -m venv sidecars/stem-sep/venv && pip install -r sidecars/stem-sep/requirements.txt' },
		'video-ai': { label: 'Video AI',  setup_hint: 'python3 -m venv sidecars/video-ai/venv && pip install -r sidecars/video-ai/requirements.txt' },
	};

	let tools: ToolStatus[] = [];
	let loading = true;

	async function refresh() {
		loading = true;
		try {
			tools = await invoke<ToolStatus[]>('check_video_tools');
		} finally {
			loading = false;
		}
	}

	onMount(refresh);
</script>

<div class="panel">
	<div class="panel-header">
		<h2>🩺 Diagnostics</h2>
		<button class="refresh-btn" on:click={refresh} disabled={loading}>{loading ? 'Checking…' : '↻ Refresh'}</button>
	</div>

	<div class="panel-body">
		<section>
			<h3>System tools (video features)</h3>
			<p class="hint">These run as plain child processes, not sidecars — checked directly on PATH.</p>
			<div class="rows">
				{#each tools as t}
					<div class="row">
						<span class="dot" class:ok={t.available} class:bad={!t.available}></span>
						<span class="name">{t.name}</span>
						<span class="used-by">{TOOL_INFO[t.name]?.used_by ?? ''}</span>
						{#if t.available}
							<span class="detail">{t.detail}</span>
						{:else}
							<span class="detail bad-text">Not found — install with: <code>{TOOL_INFO[t.name]?.install_hint}</code></span>
						{/if}
					</div>
				{/each}
				{#if tools.length === 0 && !loading}
					<p class="hint">No data yet — click Refresh.</p>
				{/if}
			</div>
		</section>

		<section>
			<h3>AI sidecars</h3>
			<p class="hint">FastAPI services, lazy-started per panel. Each needs its own Python venv (see CLAUDE.md).</p>
			<div class="rows">
				{#each $sidecarStore as sc}
					<div class="row">
						<span class="dot" class:ok={sc.status === 'running'} class:warn={sc.status === 'starting' || sc.status === 'degraded'} class:bad={sc.status === 'stopped' || sc.status === 'error'}></span>
						<span class="name">{sc.label}</span>
						<span class="used-by">:{sc.port}</span>
						<span class="detail">
							{sc.status}
							{#if sc.status === 'stopped'}
								<button class="start-btn" on:click={() => startSidecar(sc.id)}>Start</button>
							{:else if sc.status === 'error'}
								<span class="bad-text"> — failed to start. Check venv at <code>{SIDECAR_INFO[sc.id]?.setup_hint}</code></span>
							{/if}
						</span>
					</div>
				{/each}
			</div>
		</section>
	</div>
</div>

<style>
	.panel { flex: 1; display: flex; flex-direction: column; overflow: hidden; background: var(--bg-base); }

	.panel-header {
		display: flex; align-items: center; justify-content: space-between; gap: 10px;
		padding: 14px 20px 10px; border-bottom: 1px solid var(--border);
		background: var(--bg-surface); flex-shrink: 0;
	}
	.panel-header h2 { font-size: 15px; font-weight: 600; color: var(--text-primary); }

	.refresh-btn {
		font-size: 12px; padding: 4px 12px; border: 1px solid var(--border);
		border-radius: 6px; color: var(--text-secondary);
	}
	.refresh-btn:hover:not(:disabled) { background: var(--bg-highlight); color: var(--text-primary); }

	.panel-body { flex: 1; overflow-y: auto; padding: 20px; display: flex; flex-direction: column; gap: 24px; max-width: 800px; }

	h3 { font-size: 13px; font-weight: 600; color: var(--text-primary); margin-bottom: 4px; }
	.hint { font-size: 12px; color: var(--text-muted); margin-bottom: 10px; }

	.rows { display: flex; flex-direction: column; gap: 6px; }

	.row {
		display: flex; align-items: center; gap: 10px;
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: 8px; padding: 8px 14px; font-size: 12px;
	}

	.dot { width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0; }
	.dot.ok   { background: var(--success); }
	.dot.warn { background: var(--warning); }
	.dot.bad  { background: var(--error); }

	.name { font-weight: 600; min-width: 70px; color: var(--text-primary); }
	.used-by { color: var(--text-muted); min-width: 220px; }
	.detail { color: var(--text-secondary); flex: 1; }
	.bad-text { color: var(--error); }
	.detail code { background: var(--bg-highlight); padding: 1px 5px; border-radius: 3px; font-family: var(--font-mono); font-size: 11px; }

	.start-btn {
		margin-left: 8px; font-size: 11px; padding: 2px 10px; border: 1px solid var(--accent);
		border-radius: 6px; color: var(--accent-hover);
	}
	.start-btn:hover { background: var(--accent-dim); }
</style>
