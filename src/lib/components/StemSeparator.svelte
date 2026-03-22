<script lang="ts">
	import { open } from '@tauri-apps/plugin-dialog';
	import { readFile } from '@tauri-apps/plugin-fs';
	import { projectStore, addTrack } from '$lib/stores/projectStore';
	import { createTask, updateTask, completeTask, failTask } from '$lib/stores/aiTaskStore';
	import { sidecarStore, startSidecar } from '$lib/stores/sidecarStore';

	const SIDECAR_URL = 'http://127.0.0.1:8004';
	const STEM_COLORS: Record<string, string> = {
		vocals: '#e86db7',
		drums:  '#e6a817',
		bass:   '#4a90d9',
		other:  '#7c5cbf',
		piano:  '#4caf7d',
		guitar: '#e05252',
	};

	type Engine = 'demucs' | 'spleeter';
	type StemCount = 2 | 4 | 6;

	let selectedFile: string | null = null;
	let selectedFileName = '';
	let engine: Engine = 'demucs';
	let stemCount: StemCount = 4;
	let jobId: string | null = null;
	let jobStatus: 'idle' | 'running' | 'completed' | 'failed' = 'idle';
	let progress = 0;
	let stems: Record<string, string> = {};  // name → sidecar download URL
	let errorMessage = '';
	let pollInterval: ReturnType<typeof setInterval> | null = null;

	$: sidecarStatus = $sidecarStore.find((s) => s.id === 'stem-sep')?.status ?? 'stopped';
	$: sidecarReady = sidecarStatus === 'running';

	async function pickFile() {
		const result = await open({
			multiple: false,
			filters: [{ name: 'Audio', extensions: ['wav', 'mp3', 'ogg', 'flac', 'aiff', 'm4a'] }]
		});
		if (result && !Array.isArray(result)) {
			selectedFile = result;
			selectedFileName = result.split('/').pop() ?? result;
			jobStatus = 'idle';
			stems = {};
		}
	}

	async function startSeparation() {
		if (!selectedFile) return;
		if (!sidecarReady) {
			await startSidecar('stem-sep');
			// Wait briefly for startup
			await new Promise((r) => setTimeout(r, 3000));
		}

		jobStatus = 'running';
		progress = 0;
		stems = {};
		errorMessage = '';

		const taskId = createTask('stem-separation', `Separating: ${selectedFileName}`);

		try {
			// Read file via Tauri FS plugin and upload as multipart
			const fileBytes = await readFile(selectedFile);
			const formData = new FormData();
			formData.append('audio', new Blob([fileBytes]), selectedFileName);
			formData.append('engine', engine);
			formData.append('stems', String(stemCount));

			const response = await window.fetch(
				`${SIDECAR_URL}/separate?engine=${engine}&stems=${stemCount}`,
				{ method: 'POST', body: formData }
			);

			if (!response.ok) throw new Error(`HTTP ${response.status}: ${await response.text()}`);
			const data = await response.json();
			jobId = data.job_id;

			// Poll for completion
			pollInterval = setInterval(async () => {
				try {
					const statusRes = await window.fetch(`${SIDECAR_URL}/separate/${jobId}`);
					const statusData = await statusRes.json();
					progress = statusData.progress ?? 0;
					updateTask(taskId, { progress });

					if (statusData.status === 'completed') {
						clearInterval(pollInterval!);
						jobStatus = 'completed';
						stems = statusData.stems ?? {};
						completeTask(taskId);
					} else if (statusData.status === 'failed') {
						clearInterval(pollInterval!);
						jobStatus = 'failed';
						errorMessage = statusData.error ?? 'Unknown error';
						failTask(taskId, errorMessage);
					}
				} catch {
					// transient error — keep polling
				}
			}, 1500);

		} catch (e: unknown) {
			jobStatus = 'failed';
			errorMessage = String(e);
			failTask(taskId, errorMessage);
		}
	}

	async function addStemToTimeline(stemName: string, stemPath: string) {
		addTrack('audio');
		const tracks = $projectStore.tracks;
		const newTrack = tracks[tracks.length - 1];
		const color = STEM_COLORS[stemName] ?? '#7c5cbf';

		projectStore.update((p) => ({
			...p,
			dirty: true,
			tracks: p.tracks.map((t) =>
				t.id === newTrack.id
					? {
							...t,
							name: stemName,
							color,
							clips: [{
								id: crypto.randomUUID(),
								trackId: t.id,
								startBeat: 0,
								durationBeats: 128,
								filePath: stemPath,
								name: `${stemName}.wav`,
								color,
								type: 'audio' as const
							}]
					  }
					: t
			)
		}));
	}

	function addAllStems() {
		for (const [name, path] of Object.entries(stems)) {
			addStemToTimeline(name, path);
		}
	}

	function reset() {
		selectedFile = null;
		selectedFileName = '';
		jobStatus = 'idle';
		progress = 0;
		stems = {};
		jobId = null;
		if (pollInterval) { clearInterval(pollInterval); pollInterval = null; }
	}
</script>

<div class="panel">
	<div class="panel-header">
		<h2>✂ Stem Separator</h2>
		<span class="badge" class:ready={sidecarReady} class:offline={!sidecarReady}>
			{sidecarReady ? 'Ready' : 'Offline'}
		</span>
	</div>

	<div class="panel-body">
		<!-- File picker -->
		<div class="section">
			<div class="section-label">Audio File</div>
			<div class="file-row">
				<div
					class="drop-zone"
					class:has-file={!!selectedFile}
					role="button"
					tabindex="0"
					on:click={pickFile}
					on:keydown={(e) => e.key === 'Enter' && pickFile()}
				>
					{#if selectedFile}
						<span class="file-name">🎵 {selectedFileName}</span>
					{:else}
						<span>Click to pick an audio file</span>
					{/if}
				</div>
				{#if selectedFile}
					<button class="icon-btn" title="Clear" on:click={reset}>✕</button>
				{/if}
			</div>
		</div>

		<!-- Options -->
		<div class="options-row">
			<div class="option">
				<label class="section-label" for="engine-select">Engine</label>
				<select id="engine-select" bind:value={engine} disabled={jobStatus === 'running'}>
					<option value="demucs">Demucs v4 (recommended)</option>
					<option value="spleeter">Spleeter (faster)</option>
				</select>
			</div>
			<div class="option">
				<label class="section-label" for="stems-select">Stems</label>
				<select id="stems-select" bind:value={stemCount} disabled={jobStatus === 'running'}>
					<option value={2}>2 — Vocals + Other</option>
					<option value={4}>4 — Vocals / Drums / Bass / Other</option>
					{#if engine === 'demucs'}
						<option value={6}>6 — + Piano + Guitar</option>
					{/if}
				</select>
			</div>
		</div>

		<!-- Separate button -->
		<button
			class="primary-btn"
			disabled={!selectedFile || jobStatus === 'running'}
			on:click={startSeparation}
		>
			{jobStatus === 'running' ? 'Separating…' : 'Separate Stems'}
		</button>

		<!-- Progress -->
		{#if jobStatus === 'running'}
			<div class="progress-section">
				<div class="progress-bar">
					<div class="progress-fill" style="width: {progress}%"></div>
				</div>
				<span class="progress-label">{progress}%</span>
			</div>
		{/if}

		<!-- Error -->
		{#if jobStatus === 'failed'}
			<div class="error-box">
				<strong>Separation failed</strong><br />
				{errorMessage}
			</div>
		{/if}

		<!-- Results -->
		{#if jobStatus === 'completed' && Object.keys(stems).length > 0}
			<div class="results-section">
				<div class="results-header">
					<span class="section-label">Stems ready</span>
					<button class="add-all-btn" on:click={addAllStems}>+ Add All to Timeline</button>
				</div>
				<div class="stem-list">
					{#each Object.entries(stems) as [name, path]}
						<div class="stem-row">
							<span class="stem-dot" style="background: {STEM_COLORS[name] ?? '#7c5cbf'}"></span>
							<span class="stem-name">{name}</span>
							<!-- svelte-ignore a11y-media-has-caption -->
							<audio controls src="http://127.0.0.1:8004/stem/{jobId}/{name}" class="stem-audio"></audio>
							<button class="add-btn" on:click={() => addStemToTimeline(name, path)}>+ Track</button>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		<!-- Info box -->
		{#if !sidecarReady}
			<div class="info-box">
				The Stem Separation sidecar is not running. It will start automatically when you click Separate.
				Demucs and PyTorch must be installed in <code>sidecars/stem-sep/venv/</code>.
			</div>
		{/if}
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

	.panel-header h2 {
		font-size: 15px;
		font-weight: 600;
		color: var(--text-primary);
	}

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
		overflow-y: auto;
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 16px;
		max-width: 640px;
	}

	.section-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.5px;
		display: block;
		margin-bottom: 6px;
	}

	.file-row {
		display: flex;
		gap: 8px;
		align-items: center;
	}

	.drop-zone {
		flex: 1;
		border: 1px dashed var(--border);
		border-radius: 8px;
		padding: 14px 18px;
		color: var(--text-muted);
		font-size: 13px;
		cursor: pointer;
		transition: all 0.15s;
		text-align: center;
	}

	.drop-zone:hover {
		border-color: var(--accent);
		color: var(--accent-hover);
		background: var(--accent-dim);
	}

	.drop-zone.has-file {
		border-color: var(--accent);
		color: var(--text-primary);
		background: var(--bg-elevated);
	}

	.file-name { font-weight: 500; }

	.icon-btn {
		width: 28px;
		height: 28px;
		border-radius: 6px;
		font-size: 11px;
		color: var(--text-muted);
		border: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.icon-btn:hover { background: var(--error); color: #fff; border-color: var(--error); }

	.options-row {
		display: flex;
		gap: 16px;
	}

	.option {
		flex: 1;
	}

	select {
		width: 100%;
		padding: 7px 10px;
		border-radius: 6px;
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

	.progress-section {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.progress-bar {
		flex: 1;
		height: 6px;
		background: var(--bg-elevated);
		border-radius: 3px;
		overflow: hidden;
	}

	.progress-fill {
		height: 100%;
		background: var(--accent);
		border-radius: 3px;
		transition: width 0.4s ease;
	}

	.progress-label { font-size: 12px; color: var(--text-secondary); min-width: 36px; }

	.error-box {
		background: rgba(224, 82, 82, 0.1);
		border: 1px solid var(--error);
		border-radius: 8px;
		padding: 12px 16px;
		font-size: 12px;
		color: var(--error);
	}

	.results-section { display: flex; flex-direction: column; gap: 10px; }

	.results-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.add-all-btn {
		font-size: 12px;
		padding: 4px 12px;
		border: 1px solid var(--accent);
		border-radius: 6px;
		color: var(--accent-hover);
		transition: background 0.1s;
	}

	.add-all-btn:hover { background: var(--accent-dim); }

	.stem-list { display: flex; flex-direction: column; gap: 8px; }

	.stem-row {
		display: flex;
		align-items: center;
		gap: 10px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 10px 14px;
	}

	.stem-dot {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.stem-name {
		font-weight: 600;
		font-size: 13px;
		min-width: 60px;
		text-transform: capitalize;
	}

	.stem-audio {
		flex: 1;
		height: 28px;
	}

	.add-btn {
		font-size: 11px;
		padding: 4px 10px;
		border: 1px solid var(--accent);
		border-radius: 6px;
		color: var(--accent-hover);
		white-space: nowrap;
	}

	.add-btn:hover { background: var(--accent-dim); }

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
		background: var(--bg-highlight);
		padding: 1px 5px;
		border-radius: 3px;
		font-family: var(--font-mono);
		font-size: 11px;
	}
</style>
