<script lang="ts">
	import { open, save } from '@tauri-apps/plugin-dialog';
	import { readFile } from '@tauri-apps/plugin-fs';
	import { projectStore } from '$lib/stores/projectStore';
	import { createTask, completeTask, failTask, updateTask } from '$lib/stores/aiTaskStore';
	import { sidecarStore } from '$lib/stores/sidecarStore';

	const SIDECAR_URL = 'http://127.0.0.1:8002';

	// -------------------------------------------------------------------------
	// Effect definitions
	// -------------------------------------------------------------------------
	type ParamType = 'range' | 'select';

	interface EffectParam {
		name: string;        // API param name
		label: string;
		type: ParamType;
		min?: number;
		max?: number;
		step?: number;
		options?: { value: string; label: string }[];
		default: number | string;
	}

	interface EffectDef {
		type: string;
		label: string;
		icon: string;
		params: EffectParam[];
	}

	const EFFECT_DEFS: EffectDef[] = [
		{
			type: 'reverb',
			label: 'Reverb',
			icon: '🌊',
			params: [
				{ name: 'room_size',  label: 'Room Size', type: 'range', min: 0, max: 1, step: 0.01, default: 0.5 },
				{ name: 'damping',    label: 'Damping',   type: 'range', min: 0, max: 1, step: 0.01, default: 0.5 },
				{ name: 'wet_level',  label: 'Wet',       type: 'range', min: 0, max: 1, step: 0.01, default: 0.33 },
				{ name: 'dry_level',  label: 'Dry',       type: 'range', min: 0, max: 1, step: 0.01, default: 0.4 },
			]
		},
		{
			type: 'compressor',
			label: 'Compressor',
			icon: '📊',
			params: [
				{ name: 'threshold_db', label: 'Threshold', type: 'range', min: -60, max: 0,  step: 0.5,  default: -20 },
				{ name: 'ratio',        label: 'Ratio',     type: 'range', min: 1,   max: 20, step: 0.1,  default: 4 },
				{ name: 'attack_ms',    label: 'Attack ms', type: 'range', min: 0,   max: 200, step: 1,   default: 20 },
				{ name: 'release_ms',   label: 'Release ms',type: 'range', min: 10,  max: 1000, step: 10, default: 150 },
			]
		},
		{
			type: 'chorus',
			label: 'Chorus',
			icon: '🎭',
			params: [
				{ name: 'rate_hz',  label: 'Rate Hz', type: 'range', min: 0.1, max: 10, step: 0.1, default: 1.0 },
				{ name: 'depth',    label: 'Depth',   type: 'range', min: 0,   max: 1,  step: 0.01, default: 0.25 },
				{ name: 'mix',      label: 'Mix',     type: 'range', min: 0,   max: 1,  step: 0.01, default: 0.5 },
			]
		},
		{
			type: 'delay',
			label: 'Delay',
			icon: '🔁',
			params: [
				{ name: 'delay_seconds', label: 'Delay s', type: 'range', min: 0, max: 2, step: 0.01, default: 0.25 },
				{ name: 'feedback',      label: 'Feedback', type: 'range', min: 0, max: 1, step: 0.01, default: 0.35 },
				{ name: 'mix',           label: 'Mix',      type: 'range', min: 0, max: 1, step: 0.01, default: 0.4 },
			]
		},
		{
			type: 'distortion',
			label: 'Distortion',
			icon: '⚡',
			params: [
				{ name: 'drive_db', label: 'Drive dB', type: 'range', min: 0, max: 60, step: 0.5, default: 25 },
			]
		},
		{
			type: 'highpass',
			label: 'High-Pass EQ',
			icon: '↗',
			params: [
				{ name: 'cutoff_frequency_hz', label: 'Cutoff Hz', type: 'range', min: 20, max: 2000, step: 10, default: 80 },
			]
		},
		{
			type: 'lowpass',
			label: 'Low-Pass EQ',
			icon: '↘',
			params: [
				{ name: 'cutoff_frequency_hz', label: 'Cutoff Hz', type: 'range', min: 500, max: 20000, step: 100, default: 8000 },
			]
		},
	];

	// -------------------------------------------------------------------------
	// Rack state
	// -------------------------------------------------------------------------
	interface RackEffect {
		id: string;
		type: string;
		enabled: boolean;
		params: Record<string, number | string>;
	}

	let rack: RackEffect[] = [];
	let inputFile: string | null = null;
	let inputFileName = '';
	let outputFile: string | null = null;
	let processing = false;
	let resultPath: string | null = null;

	$: sidecarReady = ($sidecarStore.find((s) => s.id === 'audio-fx')?.status ?? 'stopped') === 'running';

	function addEffect(def: EffectDef) {
		const params: Record<string, number | string> = {};
		for (const p of def.params) params[p.name] = p.default;
		rack = [...rack, { id: crypto.randomUUID(), type: def.type, enabled: true, params }];
	}

	function removeEffect(id: string) {
		rack = rack.filter((e) => e.id !== id);
	}

	function moveUp(idx: number) {
		if (idx === 0) return;
		const r = [...rack];
		[r[idx - 1], r[idx]] = [r[idx], r[idx - 1]];
		rack = r;
	}

	function moveDown(idx: number) {
		if (idx === rack.length - 1) return;
		const r = [...rack];
		[r[idx], r[idx + 1]] = [r[idx + 1], r[idx]];
		rack = r;
	}

	function labelFor(effectType: string): string {
		return EFFECT_DEFS.find((d) => d.type === effectType)?.label ?? effectType;
	}

	function iconFor(effectType: string): string {
		return EFFECT_DEFS.find((d) => d.type === effectType)?.icon ?? '⚙';
	}

	async function pickInput() {
		const result = await open({
			multiple: false,
			filters: [{ name: 'Audio', extensions: ['wav', 'mp3', 'ogg', 'flac', 'aiff', 'm4a'] }]
		});
		if (result && !Array.isArray(result)) {
			inputFile = result;
			inputFileName = result.split('/').pop() ?? result;
			resultPath = null;
		}
	}

	async function renderChain() {
		if (!inputFile || rack.length === 0) return;
		processing = true;
		resultPath = null;
		const taskId = createTask('effects-render', `Rendering FX: ${inputFileName}`);

		try {
			const fileBytes = await readFile(inputFile);
			const formData = new FormData();
			formData.append('audio', new Blob([fileBytes]), inputFileName);

			const enabledEffects = rack.filter((e) => e.enabled).map((e) => ({
				type: e.type,
				params: e.params
			}));
			formData.append('effects', JSON.stringify(enabledEffects));

			const res = await window.fetch(`${SIDECAR_URL}/fx/chain`, {
				method: 'POST',
				body: formData
			});

			if (!res.ok) {
				const err = await res.text();
				throw new Error(`Sidecar error: ${err}`);
			}

			// Get the processed audio blob and create an object URL for preview
			const blob = await res.blob();
			resultPath = URL.createObjectURL(blob);
			completeTask(taskId);
		} catch (e: unknown) {
			failTask(taskId, String(e));
			alert(`Effects render failed: ${e}`);
		} finally {
			processing = false;
		}
	}
</script>

<div class="panel">
	<div class="panel-header">
		<h2>⚙ Effects Rack</h2>
		<span class="badge" class:ready={sidecarReady} class:offline={!sidecarReady}>
			{sidecarReady ? 'Ready' : 'Offline'}
		</span>
	</div>

	<div class="panel-body">
		<div class="two-col">
			<!-- LEFT: Effect palette -->
			<div class="palette">
				<div class="col-header">Add Effect</div>
				<div class="palette-list">
					{#each EFFECT_DEFS as def}
						<button class="palette-item" on:click={() => addEffect(def)}>
							<span class="fx-icon">{def.icon}</span>
							<span>{def.label}</span>
						</button>
					{/each}
				</div>
			</div>

			<!-- RIGHT: Rack + controls -->
			<div class="rack-area">
				<!-- Input file -->
				<div class="section">
					<div class="col-header">Input Audio</div>
					<div class="file-row">
						<div
							class="drop-zone"
							class:has-file={!!inputFile}
							role="button"
							tabindex="0"
							on:click={pickInput}
							on:keydown={(e) => e.key === 'Enter' && pickInput()}
						>
							{#if inputFile}
								<span>🎵 {inputFileName}</span>
							{:else}
								<span>Click to pick audio</span>
							{/if}
						</div>
					</div>
				</div>

				<!-- Rack -->
				<div class="section">
					<div class="col-header">Chain ({rack.filter((e) => e.enabled).length} active)</div>
					{#if rack.length === 0}
						<div class="empty-rack">← Add effects from the palette</div>
					{:else}
						<div class="rack">
							{#each rack as effect, i (effect.id)}
								<div class="rack-slot" class:disabled={!effect.enabled}>
									<div class="slot-header">
										<button class="slot-toggle" on:click={() => (effect.enabled = !effect.enabled)}
											title={effect.enabled ? 'Bypass' : 'Enable'}>
											{effect.enabled ? '●' : '○'}
										</button>
										<span class="slot-icon">{iconFor(effect.type)}</span>
										<span class="slot-label">{labelFor(effect.type)}</span>
										<div class="slot-actions">
											<button class="slot-btn" on:click={() => moveUp(i)} disabled={i === 0}>▲</button>
											<button class="slot-btn" on:click={() => moveDown(i)} disabled={i === rack.length - 1}>▼</button>
											<button class="slot-btn danger" on:click={() => removeEffect(effect.id)}>✕</button>
										</div>
									</div>
									{#if effect.enabled}
										<div class="slot-params">
											{#each (EFFECT_DEFS.find((d) => d.type === effect.type)?.params ?? []) as param}
												<label class="param-row">
													<span class="param-label">{param.label}</span>
													{#if param.type === 'range'}
														<input
															type="range"
															min={param.min}
															max={param.max}
															step={param.step}
															bind:value={effect.params[param.name]}
															class="param-slider"
														/>
														<span class="param-value">{Number(effect.params[param.name]).toFixed(2)}</span>
													{/if}
												</label>
											{/each}
										</div>
									{/if}
								</div>
							{/each}
						</div>
					{/if}
				</div>

				<!-- Render -->
				<button
					class="primary-btn"
					disabled={!inputFile || rack.length === 0 || processing || !sidecarReady}
					on:click={renderChain}
				>
					{processing ? 'Rendering…' : 'Render Chain'}
				</button>

				<!-- Preview result -->
				{#if resultPath}
					<div class="result-section">
						<div class="col-header">Processed Output</div>
						<!-- svelte-ignore a11y-media-has-caption -->
						<audio controls src={resultPath} class="result-audio"></audio>
					</div>
				{/if}

				{#if !sidecarReady}
					<div class="info-box">
						Audio FX sidecar (port 8002) not running. Start it from the terminal:<br />
						<code>source sidecars/audio-fx/venv/bin/activate && python3 sidecars/audio-fx/main.py</code>
					</div>
				{/if}
			</div>
		</div>
	</div>
</div>

<style>
	.panel {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		background: var(--bg-base);
	}

	.panel-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 20px 10px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-surface);
		flex-shrink: 0;
	}

	.panel-header h2 { font-size: 15px; font-weight: 600; }

	.badge {
		font-size: 10px;
		padding: 2px 8px;
		border-radius: 10px;
		border: 1px solid;
	}
	.badge.ready  { color: var(--success); border-color: var(--success); background: rgba(76,175,125,0.1); }
	.badge.offline { color: var(--text-muted); border-color: var(--border); }

	.panel-body {
		flex: 1;
		overflow: hidden;
		padding: 16px;
	}

	.two-col {
		display: flex;
		gap: 16px;
		height: 100%;
	}

	.palette {
		width: 160px;
		flex-shrink: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.palette-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.palette-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 7px 10px;
		border-radius: 6px;
		border: 1px solid var(--border);
		background: var(--bg-surface);
		color: var(--text-secondary);
		font-size: 12px;
		transition: all 0.1s;
		text-align: left;
	}

	.palette-item:hover {
		border-color: var(--accent);
		color: var(--accent-hover);
		background: var(--accent-dim);
	}

	.fx-icon { font-size: 14px; }

	.rack-area {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 14px;
		overflow-y: auto;
	}

	.col-header {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.5px;
		margin-bottom: 6px;
	}

	.section { display: flex; flex-direction: column; }

	.file-row { display: flex; }

	.drop-zone {
		flex: 1;
		border: 1px dashed var(--border);
		border-radius: 6px;
		padding: 10px 14px;
		color: var(--text-muted);
		font-size: 12px;
		cursor: pointer;
		transition: all 0.15s;
		text-align: center;
	}

	.drop-zone:hover { border-color: var(--accent); color: var(--accent-hover); background: var(--accent-dim); }
	.drop-zone.has-file { border-color: var(--accent); color: var(--text-primary); background: var(--bg-elevated); }

	.empty-rack {
		border: 1px dashed var(--border);
		border-radius: 6px;
		padding: 20px;
		text-align: center;
		color: var(--text-muted);
		font-size: 12px;
	}

	.rack { display: flex; flex-direction: column; gap: 6px; }

	.rack-slot {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
		transition: opacity 0.1s;
	}

	.rack-slot.disabled { opacity: 0.5; }

	.slot-header {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		border-bottom: 1px solid var(--border);
	}

	.slot-toggle {
		font-size: 14px;
		color: var(--accent-hover);
		width: 18px;
	}

	.slot-icon { font-size: 14px; }

	.slot-label {
		flex: 1;
		font-size: 12px;
		font-weight: 600;
	}

	.slot-actions { display: flex; gap: 4px; }

	.slot-btn {
		font-size: 10px;
		width: 20px;
		height: 20px;
		border-radius: 3px;
		border: 1px solid var(--border);
		color: var(--text-muted);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.slot-btn:hover:not(:disabled) { background: var(--bg-highlight); color: var(--text-primary); }
	.slot-btn.danger:hover { background: var(--error); color: #fff; border-color: var(--error); }
	.slot-btn:disabled { opacity: 0.3; cursor: default; }

	.slot-params {
		padding: 10px 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.param-row {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.param-label {
		font-size: 11px;
		color: var(--text-secondary);
		min-width: 80px;
	}

	.param-slider {
		flex: 1;
		height: 3px;
		accent-color: var(--accent);
		cursor: pointer;
		background: none;
		border: none;
		padding: 0;
	}

	.param-value {
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--text-muted);
		min-width: 36px;
		text-align: right;
	}

	.primary-btn {
		padding: 10px 24px;
		background: var(--accent);
		color: #fff;
		border-radius: 8px;
		font-weight: 600;
		font-size: 13px;
		transition: background 0.15s;
		align-self: flex-start;
	}

	.primary-btn:hover:not(:disabled) { background: var(--accent-hover); }
	.primary-btn:disabled { opacity: 0.4; cursor: not-allowed; }

	.result-section { display: flex; flex-direction: column; gap: 6px; }

	.result-audio { width: 100%; height: 32px; }

	.info-box {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 12px 16px;
		font-size: 12px;
		color: var(--text-secondary);
		line-height: 1.6;
	}

	.info-box code {
		display: block;
		margin-top: 6px;
		background: var(--bg-base);
		padding: 6px 10px;
		border-radius: 4px;
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--text-primary);
	}
</style>
