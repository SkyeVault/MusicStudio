<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import * as Tone from 'tone';
	import {
		midiStore, activeTrack,
		addNote, updateNote, deleteSelectedNotes, clearSelection, selectAll,
		ticksToBeats, beatsToTicks, snapTick, midiToNoteName,
		exportToMidiBytes, parseMidiBytes,
		type MidiNote
	} from '$lib/stores/midiStore';
	import { projectStore } from '$lib/stores/projectStore';
	import { open, save } from '@tauri-apps/plugin-dialog';
	import { readFile, writeFile } from '@tauri-apps/plugin-fs';

	// ---------------------------------------------------------------------------
	// Layout constants
	// ---------------------------------------------------------------------------
	const KEY_WIDTH      = 56;   // px — piano keyboard column width
	const NOTE_HEIGHT    = 14;   // px per semitone row
	const HEADER_HEIGHT  = 28;   // px — beat ruler height
	const TOTAL_PITCHES  = 128;
	const VISIBLE_BEATS  = 32;   // default visible beat range

	// Colours
	const NOTE_COLOR         = '#7c5cbf';
	const NOTE_SELECTED      = '#b388ff';
	const NOTE_HOVER         = '#9b6fe0';
	const BG_COLOR           = '#0f0f0f';
	const GRID_LINE          = '#1e1e1e';
	const BEAT_LINE          = '#2a2a2a';
	const BAR_LINE           = '#333';
	const BLACK_KEY_ROW      = '#141414';
	const WHITE_KEY_ROW      = '#181818';
	const PLAYHEAD_COLOR     = '#e86db7';

	// Piano key layout (pattern repeats every 12 semitones)
	const BLACK_KEY_PATTERN = [false, true, false, true, false, false, true, false, true, false, true, false];

	// ---------------------------------------------------------------------------
	// State
	// ---------------------------------------------------------------------------
	let canvasEl: HTMLCanvasElement;
	let ctx: CanvasRenderingContext2D;
	let containerEl: HTMLDivElement;
	let rafId: number;
	let width = 800;
	let height = 600;

	// View / scroll
	let scrollX = 0;   // horizontal scroll in pixels
	let scrollY = Math.max(0, (TOTAL_PITCHES - 72) * NOTE_HEIGHT);  // start around C4
	let pxPerBeat = 60; // zoom: pixels per beat

	// Interaction
	type Tool = 'draw' | 'select' | 'erase';
	let tool: Tool = 'draw';
	let quantize = 16; // 1/16 note
	let dragging: { noteId: string; mode: 'move' | 'resize'; startX: number; startTick: number; startDuration: number; startPitch: number } | null = null;
	let hoverNoteId: string | null = null;
	let playheadX = 0;
	let selectionRect: { x0: number; y0: number; x1: number; y1: number } | null = null;
	let mouseDown = false;

	// Sampler for preview playback
	let sampler: Tone.Sampler | null = null;

	$: ppq = $midiStore.ppq;
	$: bpm = $projectStore.bpm;
	$: track = $activeTrack;
	$: notes = track?.notes ?? [];

	// ---------------------------------------------------------------------------
	// Canvas helpers
	// ---------------------------------------------------------------------------

	function isBlack(pitch: number): boolean {
		return BLACK_KEY_PATTERN[pitch % 12];
	}

	function pitchToY(pitch: number): number {
		// pitch 127 = top, 0 = bottom
		return HEADER_HEIGHT + (TOTAL_PITCHES - 1 - pitch) * NOTE_HEIGHT - scrollY;
	}

	function yToPitch(y: number): number {
		return TOTAL_PITCHES - 1 - Math.floor((y - HEADER_HEIGHT + scrollY) / NOTE_HEIGHT);
	}

	function tickToX(tick: number): number {
		const beats = ticksToBeats(tick, ppq);
		return KEY_WIDTH + beats * pxPerBeat - scrollX;
	}

	function xToTick(x: number): number {
		const beats = (x - KEY_WIDTH + scrollX) / pxPerBeat;
		return Math.max(0, beatsToTicks(beats, ppq));
	}

	function gridTickSize(): number {
		return ppq * (4 / quantize);
	}

	// ---------------------------------------------------------------------------
	// Draw
	// ---------------------------------------------------------------------------

	function draw() {
		if (!ctx) return;
		ctx.clearRect(0, 0, width, height);

		drawGrid();
		drawNotes();
		drawPianoKeys();
		drawRuler();
		drawPlayhead();
		if (selectionRect) drawSelectionRect();
	}

	function drawGrid() {
		const gridTicks = gridTickSize();
		const startTick = Math.floor(xToTick(KEY_WIDTH) / gridTicks) * gridTicks;
		const endTick = xToTick(width) + gridTicks;

		// Horizontal pitch rows
		for (let p = 0; p < TOTAL_PITCHES; p++) {
			const y = pitchToY(p);
			if (y < HEADER_HEIGHT || y > height) continue;
			ctx.fillStyle = isBlack(p) ? BLACK_KEY_ROW : WHITE_KEY_ROW;
			ctx.fillRect(KEY_WIDTH, y, width - KEY_WIDTH, NOTE_HEIGHT);
		}

		// Vertical grid lines
		for (let tick = startTick; tick < endTick; tick += gridTicks) {
			const x = tickToX(tick);
			const beat = tick / ppq;
			const bar = Math.floor(beat / 4);
			const beatInBar = beat % 4;

			ctx.strokeStyle = beatInBar === 0 ? BAR_LINE : (Number.isInteger(beat) ? BEAT_LINE : GRID_LINE);
			ctx.lineWidth = beatInBar === 0 ? 1.5 : 0.5;
			ctx.beginPath();
			ctx.moveTo(x, HEADER_HEIGHT);
			ctx.lineTo(x, height);
			ctx.stroke();
		}

		// C note markers (octave lines)
		for (let p = 0; p < TOTAL_PITCHES; p += 12) {
			const y = pitchToY(p);
			if (y < HEADER_HEIGHT || y > height) continue;
			ctx.strokeStyle = '#2e2e2e';
			ctx.lineWidth = 1;
			ctx.beginPath();
			ctx.moveTo(KEY_WIDTH, y + NOTE_HEIGHT);
			ctx.lineTo(width, y + NOTE_HEIGHT);
			ctx.stroke();
		}
	}

	function drawNotes() {
		for (const note of notes) {
			const x = tickToX(note.startTick);
			const y = pitchToY(note.pitch);
			const w = Math.max(4, (note.durationTicks / ppq) * pxPerBeat - 1);
			const h = NOTE_HEIGHT - 1;

			if (x + w < KEY_WIDTH || x > width) continue;

			// Body
			ctx.fillStyle = note.id === hoverNoteId ? NOTE_HOVER
				: note.selected ? NOTE_SELECTED
				: NOTE_COLOR;
			ctx.beginPath();
			ctx.roundRect(x, y + 1, w, h - 1, 2);
			ctx.fill();

			// Velocity bar at bottom of note
			const velH = Math.round((note.velocity / 127) * (h - 2));
			ctx.fillStyle = 'rgba(255,255,255,0.15)';
			ctx.fillRect(x, y + h - velH, Math.min(w, 4), velH);

			// Label (only if wide enough)
			if (w > 22) {
				ctx.fillStyle = 'rgba(255,255,255,0.85)';
				ctx.font = `bold ${Math.min(10, NOTE_HEIGHT - 2)}px system-ui`;
				ctx.fillText(midiToNoteName(note.pitch), x + 3, y + NOTE_HEIGHT - 3);
			}
		}
	}

	function drawPianoKeys() {
		ctx.fillStyle = '#111';
		ctx.fillRect(0, HEADER_HEIGHT, KEY_WIDTH, height - HEADER_HEIGHT);

		for (let p = 0; p < TOTAL_PITCHES; p++) {
			const y = pitchToY(p);
			if (y + NOTE_HEIGHT < HEADER_HEIGHT || y > height) continue;

			const black = isBlack(p);
			ctx.fillStyle = black ? '#1a1a1a' : '#e0e0e0';
			ctx.fillRect(1, y + 1, black ? KEY_WIDTH - 14 : KEY_WIDTH - 2, NOTE_HEIGHT - 2);

			// Note label on C notes
			if (p % 12 === 0) {
				ctx.fillStyle = '#555';
				ctx.font = '9px system-ui';
				ctx.textAlign = 'right';
				ctx.fillText(midiToNoteName(p), KEY_WIDTH - 4, y + NOTE_HEIGHT - 3);
				ctx.textAlign = 'left';
			}
		}

		// Right border
		ctx.strokeStyle = '#333';
		ctx.lineWidth = 1;
		ctx.beginPath();
		ctx.moveTo(KEY_WIDTH, HEADER_HEIGHT);
		ctx.lineTo(KEY_WIDTH, height);
		ctx.stroke();
	}

	function drawRuler() {
		ctx.fillStyle = '#1a1a1a';
		ctx.fillRect(KEY_WIDTH, 0, width - KEY_WIDTH, HEADER_HEIGHT);

		const gridTicks = Math.max(ppq, gridTickSize());
		const startTick = Math.floor(xToTick(KEY_WIDTH) / gridTicks) * gridTicks;

		ctx.font = '10px system-ui';
		ctx.fillStyle = '#666';

		for (let tick = startTick; tick < xToTick(width) + gridTicks; tick += ppq) {
			const x = tickToX(tick);
			const beat = tick / ppq;
			const bar = Math.floor(beat / 4) + 1;
			const beatInBar = (beat % 4) + 1;

			if (Number.isInteger(beat)) {
				ctx.fillStyle = beatInBar === 1 ? '#999' : '#555';
				const label = beatInBar === 1 ? `${bar}` : `${bar}.${beatInBar}`;
				ctx.fillText(label, x + 3, HEADER_HEIGHT - 6);

				ctx.strokeStyle = beatInBar === 1 ? '#444' : '#2a2a2a';
				ctx.lineWidth = 1;
				ctx.beginPath();
				ctx.moveTo(x, beatInBar === 1 ? 0 : HEADER_HEIGHT / 2);
				ctx.lineTo(x, HEADER_HEIGHT);
				ctx.stroke();
			}
		}

		// Ruler border
		ctx.strokeStyle = '#333';
		ctx.lineWidth = 1;
		ctx.beginPath();
		ctx.moveTo(KEY_WIDTH, HEADER_HEIGHT);
		ctx.lineTo(width, HEADER_HEIGHT);
		ctx.stroke();
	}

	function drawPlayhead() {
		const transportSecs = Tone.getTransport().seconds;
		const tick = secondsToTicksLocal(transportSecs);
		const x = tickToX(tick);
		if (x < KEY_WIDTH || x > width) return;

		ctx.strokeStyle = PLAYHEAD_COLOR;
		ctx.lineWidth = 1.5;
		ctx.beginPath();
		ctx.moveTo(x, 0);
		ctx.lineTo(x, height);
		ctx.stroke();

		// Triangle head
		ctx.fillStyle = PLAYHEAD_COLOR;
		ctx.beginPath();
		ctx.moveTo(x - 5, 0);
		ctx.lineTo(x + 5, 0);
		ctx.lineTo(x, 8);
		ctx.fill();
	}

	function drawSelectionRect() {
		if (!selectionRect) return;
		const { x0, y0, x1, y1 } = selectionRect;
		ctx.strokeStyle = '#b388ff';
		ctx.lineWidth = 1;
		ctx.setLineDash([4, 3]);
		ctx.strokeRect(Math.min(x0, x1), Math.min(y0, y1), Math.abs(x1 - x0), Math.abs(y1 - y0));
		ctx.setLineDash([]);
		ctx.fillStyle = 'rgba(179,136,255,0.08)';
		ctx.fillRect(Math.min(x0, x1), Math.min(y0, y1), Math.abs(x1 - x0), Math.abs(y1 - y0));
	}

	function secondsToTicksLocal(s: number): number {
		return s * (bpm / 60) * ppq;
	}

	// ---------------------------------------------------------------------------
	// Animation loop
	// ---------------------------------------------------------------------------

	function loop() {
		draw();
		rafId = requestAnimationFrame(loop);
	}

	// ---------------------------------------------------------------------------
	// Mouse interaction
	// ---------------------------------------------------------------------------

	function noteAtPoint(x: number, y: number): MidiNote | null {
		for (let i = notes.length - 1; i >= 0; i--) {
			const n = notes[i];
			const nx = tickToX(n.startTick);
			const ny = pitchToY(n.pitch);
			const nw = Math.max(4, (n.durationTicks / ppq) * pxPerBeat);
			if (x >= nx && x <= nx + nw && y >= ny && y <= ny + NOTE_HEIGHT) return n;
		}
		return null;
	}

	function isResizeHandle(x: number, note: MidiNote): boolean {
		const nx = tickToX(note.startTick);
		const nw = Math.max(4, (note.durationTicks / ppq) * pxPerBeat);
		return x >= nx + nw - 6;
	}

	function onMouseDown(e: MouseEvent) {
		mouseDown = true;
		const rect = canvasEl.getBoundingClientRect();
		const mx = e.clientX - rect.left;
		const my = e.clientY - rect.top;
		if (mx < KEY_WIDTH || my < HEADER_HEIGHT) return;

		const hit = noteAtPoint(mx, my);

		if (tool === 'draw') {
			if (hit) {
				if (!hit.selected) {
					if (!e.shiftKey) clearSelection(track!.id);
					updateNote(track!.id, hit.id, { selected: true });
				}
				dragging = {
					noteId: hit.id,
					mode: isResizeHandle(mx, hit) ? 'resize' : 'move',
					startX: mx,
					startTick: hit.startTick,
					startDuration: hit.durationTicks,
					startPitch: hit.pitch
				};
			} else {
				// Place new note
				if (!e.shiftKey) clearSelection(track!.id);
				const rawTick = xToTick(mx);
				const startTick = snapTick(rawTick, ppq, quantize);
				const pitch = Math.max(0, Math.min(127, yToPitch(my)));
				const durTicks = gridTickSize();
				addNote(track!.id, { pitch, startTick, durationTicks: durTicks, velocity: 100 });
			}
		} else if (tool === 'select') {
			if (hit) {
				if (!hit.selected && !e.shiftKey) clearSelection(track!.id);
				updateNote(track!.id, hit.id, { selected: true });
				dragging = { noteId: hit.id, mode: 'move', startX: mx, startTick: hit.startTick, startDuration: hit.durationTicks, startPitch: hit.pitch };
			} else {
				if (!e.shiftKey) clearSelection(track!.id);
				selectionRect = { x0: mx, y0: my, x1: mx, y1: my };
			}
		} else if (tool === 'erase') {
			if (hit) {
				updateNote(track!.id, hit.id, { selected: true });
				deleteSelectedNotes(track!.id);
			}
		}
	}

	function onMouseMove(e: MouseEvent) {
		const rect = canvasEl.getBoundingClientRect();
		const mx = e.clientX - rect.left;
		const my = e.clientY - rect.top;

		const hit = noteAtPoint(mx, my);
		hoverNoteId = hit?.id ?? null;

		if (hit && tool === 'draw') {
			canvasEl.style.cursor = isResizeHandle(mx, hit) ? 'ew-resize' : 'grab';
		} else if (tool === 'select') {
			canvasEl.style.cursor = 'crosshair';
		} else if (tool === 'erase') {
			canvasEl.style.cursor = 'cell';
		} else {
			canvasEl.style.cursor = 'crosshair';
		}

		if (!mouseDown) return;

		if (dragging && track) {
			const dx = mx - dragging.startX;
			const dTicks = Math.round((dx / pxPerBeat) * ppq);

			if (dragging.mode === 'resize') {
				const newDur = Math.max(gridTickSize(), snapTick(dragging.startDuration + dTicks, ppq, quantize));
				updateNote(track.id, dragging.noteId, { durationTicks: newDur });
			} else {
				const newTick = Math.max(0, snapTick(dragging.startTick + dTicks, ppq, quantize));
				const dy = Math.round((my - (pitchToY(dragging.startPitch) + NOTE_HEIGHT / 2)) / NOTE_HEIGHT);
				const newPitch = Math.max(0, Math.min(127, dragging.startPitch - dy));
				updateNote(track.id, dragging.noteId, { startTick: newTick, pitch: newPitch });
			}
		}

		if (selectionRect) {
			selectionRect = { ...selectionRect, x1: mx, y1: my };
			// Select notes inside rect
			if (track) {
				const rx0 = Math.min(selectionRect.x0, selectionRect.x1);
				const rx1 = Math.max(selectionRect.x0, selectionRect.x1);
				const ry0 = Math.min(selectionRect.y0, selectionRect.y1);
				const ry1 = Math.max(selectionRect.y0, selectionRect.y1);
				for (const n of notes) {
					const nx = tickToX(n.startTick);
					const ny = pitchToY(n.pitch);
					const inside = nx < rx1 && nx + (n.durationTicks / ppq) * pxPerBeat > rx0 && ny < ry1 && ny + NOTE_HEIGHT > ry0;
					if (inside !== n.selected) updateNote(track.id, n.id, { selected: inside });
				}
			}
		}
	}

	function onMouseUp() {
		mouseDown = false;
		dragging = null;
		selectionRect = null;
	}

	function onWheel(e: WheelEvent) {
		e.preventDefault();
		if (e.ctrlKey || e.metaKey) {
			// Zoom
			const factor = e.deltaY > 0 ? 0.85 : 1.18;
			pxPerBeat = Math.max(20, Math.min(300, pxPerBeat * factor));
		} else if (e.shiftKey) {
			scrollX = Math.max(0, scrollX + e.deltaY);
		} else {
			scrollY = Math.max(0, Math.min(TOTAL_PITCHES * NOTE_HEIGHT - height + HEADER_HEIGHT, scrollY + e.deltaY));
		}
	}

	function onKeyDown(e: KeyboardEvent) {
		if (!track) return;
		if (e.key === 'Delete' || e.key === 'Backspace') {
			deleteSelectedNotes(track.id);
		} else if (e.key === 'a' && (e.ctrlKey || e.metaKey)) {
			e.preventDefault();
			selectAll(track.id);
		} else if (e.key === 'd' && !e.ctrlKey) {
			tool = 'draw';
		} else if (e.key === 's') {
			tool = 'select';
		} else if (e.key === 'e') {
			tool = 'erase';
		}
	}

	// ---------------------------------------------------------------------------
	// Resize observer
	// ---------------------------------------------------------------------------

	let resizeObs: ResizeObserver;

	function resize() {
		if (!containerEl) return;
		width = containerEl.clientWidth;
		height = containerEl.clientHeight;
		canvasEl.width = width;
		canvasEl.height = height;
	}

	// ---------------------------------------------------------------------------
	// MIDI import / export
	// ---------------------------------------------------------------------------

	async function importMidi() {
		const path = await open({ filters: [{ name: 'MIDI', extensions: ['mid', 'midi'] }] });
		if (!path || Array.isArray(path)) return;
		const bytes = await readFile(path);
		parseMidiBytes(bytes);
	}

	async function exportMidi() {
		const path = await save({ filters: [{ name: 'MIDI', extensions: ['mid'] }], defaultPath: 'export.mid' });
		if (!path) return;
		const bytes = exportToMidiBytes(bpm);
		await writeFile(path, bytes);
	}

	// ---------------------------------------------------------------------------
	// Lifecycle
	// ---------------------------------------------------------------------------

	onMount(() => {
		ctx = canvasEl.getContext('2d')!;
		resize();
		resizeObs = new ResizeObserver(resize);
		resizeObs.observe(containerEl);
		loop();

		// Simple piano sampler using Tone.js synth as fallback
		sampler = new Tone.Sampler({
			urls: { A4: 'A4.mp3' },
			onload: () => {}
		}).toDestination();
	});

	onDestroy(() => {
		cancelAnimationFrame(rafId);
		resizeObs?.disconnect();
		sampler?.dispose();
	});
</script>

<svelte:window on:keydown={onKeyDown} />

<div class="panel">
	<!-- Toolbar -->
	<div class="toolbar">
		<div class="tool-group">
			<button class="tool-btn" class:active={tool === 'draw'}   on:click={() => (tool = 'draw')}   title="Draw (D)">✏ Draw</button>
			<button class="tool-btn" class:active={tool === 'select'} on:click={() => (tool = 'select')} title="Select (S)">⬚ Select</button>
			<button class="tool-btn" class:active={tool === 'erase'}  on:click={() => (tool = 'erase')}  title="Erase (E)">⌫ Erase</button>
		</div>

		<div class="tool-group">
			<label class="ctrl-label" for="quantize-sel">Grid</label>
			<select id="quantize-sel" bind:value={quantize} class="ctrl-select">
				<option value={1}>1/1</option>
				<option value={2}>1/2</option>
				<option value={4}>1/4</option>
				<option value={8}>1/8</option>
				<option value={16}>1/16</option>
				<option value={32}>1/32</option>
			</select>
		</div>

		<div class="tool-group">
			<label class="ctrl-label" for="zoom-range">Zoom</label>
			<input id="zoom-range" type="range" min="20" max="240" bind:value={pxPerBeat} class="ctrl-range" />
		</div>

		<div class="separator"></div>

		<button class="tool-btn" on:click={importMidi}>⬆ Import MIDI</button>
		<button class="tool-btn" on:click={exportMidi}>⬇ Export MIDI</button>
		<button class="tool-btn" on:click={() => deleteSelectedNotes(track?.id ?? '')}>🗑 Delete</button>

		<div class="tool-group right">
			<span class="note-count">{notes.length} notes</span>
		</div>
	</div>

	<!-- Canvas -->
	<div class="canvas-container" bind:this={containerEl}>
		<canvas
			bind:this={canvasEl}
			on:mousedown={onMouseDown}
			on:mousemove={onMouseMove}
			on:mouseup={onMouseUp}
			on:mouseleave={onMouseUp}
			on:wheel={onWheel}
		></canvas>
	</div>

	<!-- Hints -->
	<div class="hints">
		<span>Scroll: wheel · Zoom: Ctrl+wheel · Scroll horizontally: Shift+wheel · Keys: D draw · S select · E erase · Del delete</span>
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

	.toolbar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 6px 10px;
		background: var(--bg-surface);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
		flex-wrap: wrap;
	}

	.tool-group {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 6px;
		border-right: 1px solid var(--border);
	}

	.tool-group:last-child, .tool-group.right {
		border-right: none;
		margin-left: auto;
	}

	.tool-btn {
		padding: 4px 10px;
		border-radius: 5px;
		font-size: 11px;
		color: var(--text-secondary);
		border: 1px solid transparent;
		transition: all 0.1s;
		white-space: nowrap;
	}

	.tool-btn:hover { background: var(--bg-highlight); color: var(--text-primary); }
	.tool-btn.active { background: var(--accent-dim); color: var(--accent-hover); border-color: var(--accent); }

	.ctrl-label {
		font-size: 10px;
		color: var(--text-muted);
		white-space: nowrap;
	}

	.ctrl-select {
		font-size: 11px;
		padding: 3px 6px;
		height: 26px;
	}

	.ctrl-range {
		width: 80px;
		height: 3px;
		accent-color: var(--accent);
		cursor: pointer;
		background: none;
		border: none;
		padding: 0;
	}

	.separator {
		width: 1px;
		height: 20px;
		background: var(--border);
		margin: 0 4px;
	}

	.note-count {
		font-size: 11px;
		color: var(--text-muted);
		font-family: var(--font-mono);
	}

	.canvas-container {
		flex: 1;
		overflow: hidden;
		position: relative;
	}

	canvas {
		display: block;
		width: 100%;
		height: 100%;
	}

	.hints {
		padding: 4px 10px;
		font-size: 10px;
		color: var(--text-muted);
		background: var(--bg-surface);
		border-top: 1px solid var(--border);
		flex-shrink: 0;
	}
</style>
