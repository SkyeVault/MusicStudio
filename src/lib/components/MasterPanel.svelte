<script lang="ts">
	import { open, save } from '@tauri-apps/plugin-dialog';
	import { readFile, writeFile } from '@tauri-apps/plugin-fs';
	import { sidecarStore } from '$lib/stores/sidecarStore';
	import { createTask, completeTask, failTask, updateTask } from '$lib/stores/aiTaskStore';
	import { projectStore } from '$lib/stores/projectStore';

	const SIDECAR_URL = 'http://127.0.0.1:8002';

	let targetFile: string | null = null;
	let targetName = '';
	let referenceFile: string | null = null;
	let referenceName = '';
	let resultUrl: string | null = null;
	let resultBlob: Blob | null = null;
	let status: 'idle' | 'running' | 'done' | 'error' = 'idle';
	let errorMsg = '';

	// Analysis results
	let bpmResult: number | null = null;
	let chordResult: string | null = null;
	let analyzeStatus: 'idle' | 'running' | 'done' | 'error' = 'idle';

	$: sidecarReady = ($sidecarStore.find((s) => s.id === 'audio-fx')?.status ?? 'stopped') === 'running';

	async function pickTarget() {
		const r = await open({ multiple: false, filters: [{ name: 'Audio', extensions: ['wav', 'mp3', 'ogg', 'flac'] }] });
		if (r && !Array.isArray(r)) { targetFile = r; targetName = r.split('/').pop() ?? r; resultUrl = null; }
	}

	async function pickReference() {
		const r = await open({ multiple: false, filters: [{ name: 'Audio', extensions: ['wav', 'mp3', 'ogg', 'flac'] }] });
		if (r && !Array.isArray(r)) { referenceFile = r; referenceName = r.split('/').pop() ?? r; }
	}

	async function runMastering() {
		if (!targetFile || !referenceFile) return;
		status = 'running';
		errorMsg = '';
		resultUrl = null;
		const taskId = createTask('master', `Mastering: ${targetName}`);

		try {
			const [targetBytes, refBytes] = await Promise.all([readFile(targetFile), readFile(referenceFile)]);
			const form = new FormData();
			form.append('target', new Blob([targetBytes]), targetName);
			form.append('reference', new Blob([refBytes]), referenceName);

			updateTask(taskId, { progress: 30 });

			const res = await window.fetch(`${SIDECAR_URL}/fx/master`, { method: 'POST', body: form });
			if (!res.ok) throw new Error(`${res.status}: ${await res.text()}`);

			updateTask(taskId, { progress: 90 });
			resultBlob = await res.blob();
			resultUrl = URL.createObjectURL(resultBlob);
			completeTask(taskId);
			status = 'done';
		} catch (e: unknown) {
			errorMsg = String(e);
			failTask(taskId, errorMsg);
			status = 'error';
		}
	}

	async function saveResult() {
		if (!resultBlob) return;
		const path = await save({ filters: [{ name: 'WAV', extensions: ['wav'] }], defaultPath: 'mastered.wav' });
		if (!path) return;
		const bytes = new Uint8Array(await resultBlob.arrayBuffer());
		await writeFile(path, bytes);
	}

	async function analyzeAudio() {
		if (!targetFile) return;
		analyzeStatus = 'running';
		bpmResult = null;
		chordResult = null;

		try {
			const bytes = await readFile(targetFile);
			const form = new FormData();
			form.append('audio', new Blob([bytes]), targetName);

			const [bpmRes, chordRes] = await Promise.all([
				window.fetch(`${SIDECAR_URL}/analyze/bpm`, { method: 'POST', body: form }),
				window.fetch(`${SIDECAR_URL}/analyze/chords`, { method: 'POST', body: form })
			]);

			if (bpmRes.ok) {
				const data = await bpmRes.json();
				bpmResult = Math.round(data.bpm);
			}

			if (chordRes.ok) {
				const data = await chordRes.json();
				chordResult = data.dominant_note;
			}

			analyzeStatus = 'done';
		} catch {
			analyzeStatus = 'error';
		}
	}
</script>

<div class="panel">
	<div class="panel-header">
		<h2>💿 Master</h2>
		<span class="badge" class:ready={sidecarReady} class:offline={!sidecarReady}>
			{sidecarReady ? 'Ready' : 'Offline'}
		</span>
	</div>

	<div class="panel-body">
		<!-- Mastering section -->
		<div class="card">
			<div class="card-title">Reference Mastering <span class="tag">Matchering 2.0</span></div>
			<div class="card-desc">Matches your mix's loudness, frequency response, and stereo width to a professional reference track.</div>

			<div class="file-grid">
				<div class="file-slot">
					<div class="file-label">Your Mix (target)</div>
					<div
						class="drop-zone"
						class:has-file={!!targetFile}
						role="button" tabindex="0"
						on:click={pickTarget}
						on:keydown={(e) => e.key === 'Enter' && pickTarget()}
					>
						{targetFile ? `🎵 ${targetName}` : 'Click to pick target'}
					</div>
				</div>

				<div class="file-slot">
					<div class="file-label">Reference Track</div>
					<div
						class="drop-zone"
						class:has-file={!!referenceFile}
						role="button" tabindex="0"
						on:click={pickReference}
						on:keydown={(e) => e.key === 'Enter' && pickReference()}
					>
						{referenceFile ? `🎵 ${referenceName}` : 'Click to pick reference'}
					</div>
				</div>
			</div>

			<div class="action-row">
				<button
					class="primary-btn"
					disabled={!targetFile || !referenceFile || status === 'running' || !sidecarReady}
					on:click={runMastering}
				>
					{status === 'running' ? '⏳ Mastering…' : '💿 Master Mix'}
				</button>
				{#if status === 'done'}
					<button class="secondary-btn" on:click={saveResult}>⬇ Save WAV</button>
				{/if}
			</div>

			{#if status === 'error'}
				<div class="error-box">{errorMsg}</div>
			{/if}

			{#if resultUrl}
				<div class="preview-section">
					<div class="preview-label">Mastered output</div>
					<!-- svelte-ignore a11y-media-has-caption -->
					<audio controls src={resultUrl} class="audio-player"></audio>
				</div>
			{/if}
		</div>

		<!-- Analysis section -->
		<div class="card">
			<div class="card-title">Audio Analysis <span class="tag">librosa</span></div>
			<div class="card-desc">Detect BPM and key from any audio file.</div>

			<div class="action-row">
				<button
					class="primary-btn"
					disabled={!targetFile || analyzeStatus === 'running' || !sidecarReady}
					on:click={analyzeAudio}
				>
					{analyzeStatus === 'running' ? '⏳ Analyzing…' : '🔍 Analyze Audio'}
				</button>
				{#if !targetFile}
					<span class="hint">Pick a target file above first</span>
				{/if}
			</div>

			{#if analyzeStatus === 'done'}
				<div class="analysis-results">
					{#if bpmResult !== null}
						<div class="analysis-chip">
							<span class="chip-label">BPM</span>
							<span class="chip-value">{bpmResult}</span>
						</div>
					{/if}
					{#if chordResult}
						<div class="analysis-chip">
							<span class="chip-label">Key</span>
							<span class="chip-value">{chordResult}</span>
						</div>
					{/if}
				</div>
			{/if}
		</div>

		{#if !sidecarReady}
			<div class="info-box">
				Audio FX sidecar (port 8002) not running. Install deps and start it:<br />
				<code>source sidecars/audio-fx/venv/bin/activate && pip install matchering librosa && python3 sidecars/audio-fx/main.py</code>
			</div>
		{/if}
	</div>
</div>

<style>
	.panel { flex: 1; display: flex; flex-direction: column; overflow: hidden; background: var(--bg-base); }

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

	.badge { font-size: 10px; padding: 2px 8px; border-radius: 10px; border: 1px solid; }
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

	.card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 18px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.card-title {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.tag {
		font-size: 9px;
		padding: 2px 7px;
		background: var(--bg-highlight);
		border-radius: 8px;
		color: var(--text-muted);
		font-weight: 400;
		font-family: var(--font-mono);
	}

	.card-desc { font-size: 12px; color: var(--text-muted); line-height: 1.5; }

	.file-grid { display: flex; gap: 12px; }

	.file-slot { flex: 1; display: flex; flex-direction: column; gap: 5px; }

	.file-label { font-size: 11px; color: var(--text-secondary); font-weight: 600; }

	.drop-zone {
		border: 1px dashed var(--border);
		border-radius: 7px;
		padding: 12px;
		color: var(--text-muted);
		font-size: 12px;
		cursor: pointer;
		transition: all 0.15s;
		text-align: center;
	}

	.drop-zone:hover { border-color: var(--accent); color: var(--accent-hover); background: var(--accent-dim); }
	.drop-zone.has-file { border-color: var(--accent); color: var(--text-primary); background: var(--bg-elevated); }

	.action-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }

	.primary-btn {
		padding: 9px 20px;
		background: var(--accent);
		color: #fff;
		border-radius: 7px;
		font-weight: 600;
		font-size: 13px;
		transition: background 0.15s;
	}

	.primary-btn:hover:not(:disabled) { background: var(--accent-hover); }
	.primary-btn:disabled { opacity: 0.4; cursor: not-allowed; }

	.secondary-btn {
		padding: 9px 20px;
		border: 1px solid var(--accent);
		color: var(--accent-hover);
		border-radius: 7px;
		font-weight: 600;
		font-size: 13px;
		transition: all 0.15s;
	}

	.secondary-btn:hover { background: var(--accent-dim); }

	.hint { font-size: 11px; color: var(--text-muted); }

	.error-box {
		background: rgba(224,82,82,0.08);
		border: 1px solid var(--error);
		border-radius: 7px;
		padding: 10px 14px;
		font-size: 12px;
		color: var(--error);
	}

	.preview-section { display: flex; flex-direction: column; gap: 6px; }

	.preview-label { font-size: 11px; color: var(--text-secondary); font-weight: 600; }

	.audio-player { width: 100%; height: 32px; }

	.analysis-results { display: flex; gap: 12px; flex-wrap: wrap; }

	.analysis-chip {
		display: flex;
		align-items: center;
		gap: 8px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 20px;
		padding: 6px 14px;
	}

	.chip-label { font-size: 10px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.5px; }
	.chip-value { font-size: 18px; font-weight: 700; font-family: var(--font-mono); color: var(--accent-hover); }

	.info-box {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 12px 16px;
		font-size: 12px;
		color: var(--text-secondary);
		line-height: 1.7;
	}

	.info-box code {
		display: block;
		margin-top: 6px;
		background: var(--bg-base);
		padding: 6px 10px;
		border-radius: 4px;
		font-family: var(--font-mono);
		font-size: 10px;
	}
</style>
