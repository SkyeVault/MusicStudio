<script lang="ts">
	import { onMount } from 'svelte';
	import { open } from '@tauri-apps/plugin-dialog';
	import { readFile } from '@tauri-apps/plugin-fs';
	import { projectStore, addTrack, addClipToTrack } from '$lib/stores/projectStore';
	import { createTask, updateTask, completeTask, failTask } from '$lib/stores/aiTaskStore';
	import { sidecarStore, startSidecar } from '$lib/stores/sidecarStore';

	const SIDECAR_URL = 'http://127.0.0.1:8001';

	interface VoiceModel { name: string; has_index: boolean; }
	interface StatusInfo { rvc_installed: boolean; models_dir: string; message: string | null; }

	let status: StatusInfo | null = null;
	let models: VoiceModel[] = [];
	let modelsLoading = true;

	// Import-model form state
	let importName = '';
	let importPthPath: string | null = null;
	let importIndexPath: string | null = null;
	let importError = '';
	let importing = false;

	// Convert form state
	let selectedFile: string | null = null;
	let selectedFileName = '';
	let selectedModel = '';
	let pitchShift = 0;
	let indexRate = 0.5;
	let jobStatus: 'idle' | 'running' | 'completed' | 'failed' = 'idle';
	let progress = 0;
	let jobId: string | null = null;
	let errorMessage = '';

	$: sidecarStatus = $sidecarStore.find((s) => s.id === 'voice')?.status ?? 'stopped';
	$: sidecarReady = sidecarStatus === 'running';

	async function ensureSidecar() {
		if (!sidecarReady) {
			await startSidecar('voice');
			await new Promise((r) => setTimeout(r, 3000));
		}
	}

	async function refresh() {
		modelsLoading = true;
		try {
			const statusRes = await window.fetch(`${SIDECAR_URL}/status`);
			status = await statusRes.json();
			const modelsRes = await window.fetch(`${SIDECAR_URL}/models`);
			const data = await modelsRes.json();
			models = data.models ?? [];
			if (!selectedModel && models.length > 0) selectedModel = models[0].name;
		} catch {
			// Sidecar likely not started yet — fine, user can click Start.
		} finally {
			modelsLoading = false;
		}
	}

	onMount(async () => {
		if (sidecarReady) await refresh();
	});

	$: if (sidecarReady && modelsLoading) refresh();

	async function pickPthFile() {
		const result = await open({ multiple: false, filters: [{ name: 'RVC Model', extensions: ['pth'] }] });
		if (result && !Array.isArray(result)) {
			importPthPath = result;
			if (!importName) importName = result.split('/').pop()?.replace(/\.pth$/, '') ?? '';
		}
	}

	async function pickIndexFile() {
		const result = await open({ multiple: false, filters: [{ name: 'RVC Index', extensions: ['index'] }] });
		if (result && !Array.isArray(result)) importIndexPath = result;
	}

	async function importModel() {
		if (!importPthPath || !importName) return;
		await ensureSidecar();
		importing = true;
		importError = '';
		try {
			const res = await window.fetch(`${SIDECAR_URL}/models/import`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ name: importName, pth_path: importPthPath, index_path: importIndexPath })
			});
			if (!res.ok) throw new Error((await res.json()).detail ?? `HTTP ${res.status}`);
			importName = '';
			importPthPath = null;
			importIndexPath = null;
			await refresh();
		} catch (e) {
			importError = String(e);
		} finally {
			importing = false;
		}
	}

	async function deleteModel(name: string) {
		await window.fetch(`${SIDECAR_URL}/models/${name}`, { method: 'DELETE' });
		if (selectedModel === name) selectedModel = '';
		await refresh();
	}

	async function pickAudioFile() {
		const result = await open({
			multiple: false,
			filters: [{ name: 'Audio', extensions: ['wav', 'mp3', 'ogg', 'flac', 'aiff', 'm4a'] }]
		});
		if (result && !Array.isArray(result)) {
			selectedFile = result;
			selectedFileName = result.split('/').pop() ?? result;
			jobStatus = 'idle';
		}
	}

	async function startConversion() {
		if (!selectedFile || !selectedModel) return;
		await ensureSidecar();

		jobStatus = 'running';
		progress = 0;
		errorMessage = '';
		const taskId = createTask('voice-convert', `Converting: ${selectedFileName} → ${selectedModel}`);

		try {
			const fileBytes = await readFile(selectedFile);
			const formData = new FormData();
			formData.append('audio', new Blob([fileBytes]), selectedFileName);
			formData.append('model_name', selectedModel);
			formData.append('pitch_shift', String(pitchShift));
			formData.append('index_rate', String(indexRate));

			const response = await window.fetch(`${SIDECAR_URL}/convert`, { method: 'POST', body: formData });
			if (!response.ok) throw new Error(`HTTP ${response.status}: ${await response.text()}`);
			const data = await response.json();
			jobId = data.job_id;

			const poll = setInterval(async () => {
				try {
					const statusRes = await window.fetch(`${SIDECAR_URL}/convert/${jobId}`);
					const statusData = await statusRes.json();
					progress = statusData.progress ?? 0;
					updateTask(taskId, { progress });

					if (statusData.status === 'completed') {
						clearInterval(poll);
						jobStatus = 'completed';
						completeTask(taskId);
					} else if (statusData.status === 'failed') {
						clearInterval(poll);
						jobStatus = 'failed';
						errorMessage = statusData.error ?? 'Unknown error';
						failTask(taskId, errorMessage);
					}
				} catch {
					// transient — keep polling
				}
			}, 1500);
		} catch (e) {
			jobStatus = 'failed';
			errorMessage = String(e);
			failTask(taskId, errorMessage);
		}
	}

	function addResultToTimeline() {
		if (!jobId) return;
		addTrack('audio');
		const tracks = $projectStore.tracks;
		const newTrack = tracks[tracks.length - 1];
		const resultUrl = `${SIDECAR_URL}/convert/${jobId}/result`;

		addClipToTrack(newTrack.id, {
			startBeat: 0,
			durationBeats: 128,
			filePath: resultUrl,
			name: `${selectedModel} (converted)`,
			color: newTrack.color,
			type: 'audio'
		});
	}
</script>

<div class="panel">
	<div class="panel-header">
		<h2>🎤 Voice Conversion</h2>
		<span class="badge" class:ready={sidecarReady} class:offline={!sidecarReady}>
			{sidecarReady ? 'Ready' : 'Offline'}
		</span>
	</div>

	<div class="panel-body">
		{#if status && !status.rvc_installed}
			<div class="info-box warn">
				<strong>RVC is not installed yet.</strong> {status.message}
			</div>
		{/if}

		<section>
			<div class="section-label">Voice Models</div>
			<p class="hint">
				Each voice needs its own trained <code>.pth</code> (+ optional <code>.index</code>) checkpoint —
				there's no generic "voice cloning model" to download. Import one you already have, or train/download
				one elsewhere first.
			</p>

			{#if models.length > 0}
				<div class="model-list">
					{#each models as m}
						<div class="model-row">
							<span class="model-name">{m.name}</span>
							{#if m.has_index}<span class="index-badge">+index</span>{/if}
							<button class="icon-btn" title="Delete model" on:click={() => deleteModel(m.name)}>✕</button>
						</div>
					{/each}
				</div>
			{:else}
				<p class="hint">No voice models installed yet.</p>
			{/if}

			<div class="import-form">
				<input class="name-input" type="text" placeholder="Voice name" bind:value={importName} />
				<button class="file-btn" on:click={pickPthFile}>{importPthPath ? `✓ ${importPthPath.split('/').pop()}` : 'Pick .pth file'}</button>
				<button class="file-btn" on:click={pickIndexFile}>{importIndexPath ? `✓ ${importIndexPath.split('/').pop()}` : 'Pick .index file (optional)'}</button>
				<button class="primary-btn small" disabled={!importPthPath || !importName || importing} on:click={importModel}>
					{importing ? 'Importing…' : '+ Import Model'}
				</button>
			</div>
			{#if importError}
				<div class="error-box">{importError}</div>
			{/if}
		</section>

		<section>
			<div class="section-label">Convert Audio</div>
			<div class="file-row">
				<div class="drop-zone" class:has-file={!!selectedFile} role="button" tabindex="0"
					on:click={pickAudioFile} on:keydown={(e) => e.key === 'Enter' && pickAudioFile()}>
					{#if selectedFile}
						<span class="file-name">🎵 {selectedFileName}</span>
					{:else}
						<span>Click to pick an audio file to convert</span>
					{/if}
				</div>
			</div>

			<div class="options-row">
				<div class="option">
					<label class="section-label" for="model-select">Voice Model</label>
					<select id="model-select" bind:value={selectedModel} disabled={jobStatus === 'running' || models.length === 0}>
						{#each models as m}
							<option value={m.name}>{m.name}</option>
						{/each}
					</select>
				</div>
				<div class="option">
					<label class="section-label" for="pitch-input">Pitch shift (semitones)</label>
					<input id="pitch-input" type="number" min="-24" max="24" bind:value={pitchShift} disabled={jobStatus === 'running'} />
				</div>
				<div class="option">
					<label class="section-label" for="index-rate-input">Index rate</label>
					<input id="index-rate-input" type="number" min="0" max="1" step="0.05" bind:value={indexRate} disabled={jobStatus === 'running'} />
				</div>
			</div>

			<button class="primary-btn" disabled={!selectedFile || !selectedModel || jobStatus === 'running'} on:click={startConversion}>
				{jobStatus === 'running' ? 'Converting…' : 'Convert'}
			</button>

			{#if jobStatus === 'running'}
				<div class="progress-section">
					<div class="progress-bar"><div class="progress-fill" style="width: {progress}%"></div></div>
					<span class="progress-label">{progress}%</span>
				</div>
			{/if}

			{#if jobStatus === 'failed'}
				<div class="error-box"><strong>Conversion failed</strong><br />{errorMessage}</div>
			{/if}

			{#if jobStatus === 'completed' && jobId}
				<div class="results-section">
					<!-- svelte-ignore a11y-media-has-caption -->
					<audio controls src="{SIDECAR_URL}/convert/{jobId}/result" class="result-audio"></audio>
					<button class="add-btn" on:click={addResultToTimeline}>+ Add to Timeline</button>
				</div>
			{/if}
		</section>

		{#if !sidecarReady}
			<div class="info-box">
				The Voice sidecar is not running. It will start automatically when you import a model or convert audio.
				rvc-python must be installed in <code>sidecars/voice/venv/</code> (Python 3.11 — see CLAUDE.md).
			</div>
		{/if}
	</div>
</div>

<style>
	.panel { flex: 1; display: flex; flex-direction: column; overflow: hidden; background: var(--bg-base); }

	.panel-header {
		display: flex; align-items: center; gap: 10px;
		padding: 14px 20px 10px; border-bottom: 1px solid var(--border);
		background: var(--bg-surface); flex-shrink: 0;
	}
	.panel-header h2 { font-size: 15px; font-weight: 600; color: var(--text-primary); }

	.badge { font-size: 10px; padding: 2px 8px; border-radius: 10px; border: 1px solid; }
	.badge.ready { color: var(--success); border-color: var(--success); background: rgba(76,175,125,0.1); }
	.badge.offline { color: var(--text-muted); border-color: var(--border); }

	.panel-body {
		flex: 1; overflow-y: auto; padding: 20px;
		display: flex; flex-direction: column; gap: 24px; max-width: 720px;
	}

	section { display: flex; flex-direction: column; gap: 10px; }

	.section-label {
		font-size: 11px; font-weight: 600; color: var(--text-secondary);
		text-transform: uppercase; letter-spacing: 0.5px; display: block; margin-bottom: 2px;
	}

	.hint { font-size: 12px; color: var(--text-muted); line-height: 1.5; }
	.hint code { background: var(--bg-highlight); padding: 1px 5px; border-radius: 3px; font-family: var(--font-mono); font-size: 11px; }

	.model-list { display: flex; flex-direction: column; gap: 6px; }

	.model-row {
		display: flex; align-items: center; gap: 10px;
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: 8px; padding: 8px 14px; font-size: 12px;
	}
	.model-name { font-weight: 600; color: var(--text-primary); flex: 1; }
	.index-badge { font-size: 10px; color: var(--accent-hover); border: 1px solid var(--accent); border-radius: 8px; padding: 1px 8px; }

	.icon-btn {
		width: 24px; height: 24px; border-radius: 6px; font-size: 11px;
		color: var(--text-muted); border: 1px solid var(--border);
		display: flex; align-items: center; justify-content: center;
	}
	.icon-btn:hover { background: var(--error); color: #fff; border-color: var(--error); }

	.import-form { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }

	.name-input {
		padding: 7px 10px; border-radius: 6px; border: 1px solid var(--border);
		background: var(--bg-surface); color: var(--text-primary); font-size: 12px; width: 140px;
	}

	.file-btn {
		font-size: 12px; padding: 7px 12px; border: 1px dashed var(--border);
		border-radius: 6px; color: var(--text-muted);
	}
	.file-btn:hover { border-color: var(--accent); color: var(--accent-hover); }

	.file-row { display: flex; gap: 8px; align-items: center; }

	.drop-zone {
		flex: 1; border: 1px dashed var(--border); border-radius: 8px;
		padding: 14px 18px; color: var(--text-muted); font-size: 13px;
		cursor: pointer; transition: all 0.15s; text-align: center;
	}
	.drop-zone:hover { border-color: var(--accent); color: var(--accent-hover); background: var(--accent-dim); }
	.drop-zone.has-file { border-color: var(--accent); color: var(--text-primary); background: var(--bg-elevated); }
	.file-name { font-weight: 500; }

	.options-row { display: flex; gap: 16px; }
	.option { flex: 1; }
	.option input, .option select { width: 100%; padding: 7px 10px; border-radius: 6px; }

	.primary-btn {
		padding: 10px 24px; background: var(--accent); color: #fff; border-radius: 8px;
		font-weight: 600; font-size: 13px; transition: background 0.15s; align-self: flex-start;
	}
	.primary-btn.small { padding: 7px 14px; font-size: 12px; }
	.primary-btn:hover:not(:disabled) { background: var(--accent-hover); }
	.primary-btn:disabled { opacity: 0.4; cursor: not-allowed; }

	.progress-section { display: flex; align-items: center; gap: 12px; }
	.progress-bar { flex: 1; height: 6px; background: var(--bg-elevated); border-radius: 3px; overflow: hidden; }
	.progress-fill { height: 100%; background: var(--accent); border-radius: 3px; transition: width 0.4s ease; }
	.progress-label { font-size: 12px; color: var(--text-secondary); min-width: 36px; }

	.error-box {
		background: rgba(224, 82, 82, 0.1); border: 1px solid var(--error);
		border-radius: 8px; padding: 12px 16px; font-size: 12px; color: var(--error);
	}

	.results-section { display: flex; align-items: center; gap: 12px; }
	.result-audio { flex: 1; height: 32px; }

	.add-btn {
		font-size: 12px; padding: 6px 14px; border: 1px solid var(--accent);
		border-radius: 6px; color: var(--accent-hover); white-space: nowrap;
	}
	.add-btn:hover { background: var(--accent-dim); }

	.info-box {
		background: var(--bg-elevated); border: 1px solid var(--border); border-radius: 8px;
		padding: 12px 16px; font-size: 12px; color: var(--text-secondary); line-height: 1.6;
	}
	.info-box.warn { border-color: var(--warning); color: var(--warning); background: rgba(230, 168, 23, 0.08); }
	.info-box code { background: var(--bg-highlight); padding: 1px 5px; border-radius: 3px; font-family: var(--font-mono); font-size: 11px; }
</style>
