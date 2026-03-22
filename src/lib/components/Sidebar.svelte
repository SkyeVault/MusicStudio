<script lang="ts">
	import { sidecarStore, startSidecar, type SidecarId } from '$lib/stores/sidecarStore';
	import { activePanelStore, setActivePanel, type PanelId } from '$lib/stores/uiStore';

	const panels: {
		id: PanelId;
		icon: string;
		label: string;
		sidecar?: SidecarId;
		group?: string;
	}[] = [
		// Arrange
		{ id: 'timeline',     icon: '◼',  label: 'Timeline',   group: 'arrange' },
		{ id: 'stem-sep',     icon: '✂',  label: 'Stems',      group: 'arrange', sidecar: 'stem-sep' },
		// MIDI
		{ id: 'piano-roll',   icon: '🎹', label: 'Piano Roll', group: 'midi' },
		{ id: 'transcribe',   icon: '🎼', label: 'Transcribe', group: 'midi',    sidecar: 'audio-fx' },
		// FX
		{ id: 'fx-rack',      icon: '⚙',  label: 'FX Rack',   group: 'fx',      sidecar: 'audio-fx' },
		{ id: 'master',       icon: '💿', label: 'Master',     group: 'fx',      sidecar: 'audio-fx' },
		// AI Generation
		{ id: 'voice',        icon: '🎤', label: 'Voice',      group: 'ai',      sidecar: 'voice' },
		{ id: 'song-factory', icon: '✨', label: 'Song',       group: 'ai',      sidecar: 'song-gen' },
		{ id: 'backing',      icon: '🎵', label: 'Backing',    group: 'ai',      sidecar: 'song-gen' },
		// System
		{ id: 'models',       icon: '📦', label: 'Models',     group: 'system' },
	];

	function statusColor(status: string): string {
		switch (status) {
			case 'running':  return 'var(--success)';
			case 'starting': return 'var(--warning)';
			case 'error':
			case 'degraded': return 'var(--error)';
			default:         return 'var(--text-muted)';
		}
	}

	function getSidecarStatus(sidecarId: SidecarId | undefined): string {
		if (!sidecarId) return 'n/a';
		return $sidecarStore.find((s) => s.id === sidecarId)?.status ?? 'stopped';
	}

	async function selectPanel(panel: typeof panels[0]) {
		setActivePanel(panel.id);
		if (panel.sidecar) {
			const status = getSidecarStatus(panel.sidecar);
			if (status === 'stopped') startSidecar(panel.sidecar);
		}
	}

	// Group labels
	const GROUP_LABELS: Record<string, string> = {
		arrange: 'Arrange',
		midi:    'MIDI',
		fx:      'Effects',
		ai:      'AI Gen',
		system:  'System',
	};

	// Build grouped structure
	const groups = [...new Set(panels.map((p) => p.group ?? 'other'))];
</script>

<aside class="sidebar">
	{#each groups as group}
		<div class="group-label">{GROUP_LABELS[group] ?? group}</div>
		{#each panels.filter((p) => (p.group ?? 'other') === group) as panel}
			<button
				class="nav-item"
				class:active={$activePanelStore === panel.id}
				title={panel.label}
				on:click={() => selectPanel(panel)}
			>
				<span class="nav-icon">{panel.icon}</span>
				<span class="nav-label">{panel.label}</span>
				{#if panel.sidecar}
					<span
						class="status-dot"
						style="background: {statusColor(getSidecarStatus(panel.sidecar))}"
					></span>
				{/if}
			</button>
		{/each}
	{/each}

	<div class="sidebar-footer">
		{#each $sidecarStore as sc}
			<div class="sc-row">
				<span class="sc-dot" style="background: {statusColor(sc.status)}"></span>
				<span class="sc-label">{sc.label}</span>
				<span class="sc-port">:{sc.port}</span>
			</div>
		{/each}
	</div>
</aside>

<style>
	.sidebar {
		width: 80px;
		background: var(--bg-surface);
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		padding: 6px 4px;
		flex-shrink: 0;
		overflow-y: auto;
	}

	.group-label {
		font-size: 8px;
		font-weight: 700;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.8px;
		padding: 8px 6px 2px;
	}

	.nav-item {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 2px;
		padding: 7px 4px;
		border-radius: 6px;
		color: var(--text-muted);
		transition: background 0.1s, color 0.1s;
		position: relative;
		width: 100%;
	}

	.nav-item:hover { background: var(--bg-elevated); color: var(--text-secondary); }
	.nav-item.active { background: var(--accent-dim); color: var(--accent-hover); }

	.nav-icon { font-size: 15px; line-height: 1; }
	.nav-label { font-size: 9px; white-space: nowrap; }

	.status-dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		position: absolute;
		top: 5px;
		right: 5px;
	}

	.sidebar-footer {
		margin-top: auto;
		border-top: 1px solid var(--border);
		padding-top: 6px;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.sc-row {
		display: flex;
		align-items: center;
		gap: 3px;
		padding: 0 4px;
		font-size: 8px;
		color: var(--text-muted);
	}

	.sc-dot { width: 4px; height: 4px; border-radius: 50%; flex-shrink: 0; }
	.sc-label { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.sc-port { font-family: var(--font-mono); font-size: 7px; }
</style>
