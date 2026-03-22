/**
 * Audio engine wrapper around Tone.js.
 * Manages the master transport, per-track players, and effects routing.
 */
import * as Tone from 'tone';

export interface PlayerHandle {
	player: Tone.Player;
	channel: Tone.Channel;
}

class AudioEngine {
	private players: Map<string, PlayerHandle> = new Map();
	private initialized = false;

	async init() {
		if (this.initialized) return;
		await Tone.start();
		this.initialized = true;
	}

	private get transport() {
		return typeof window !== 'undefined' ? Tone.getTransport() : null;
	}

	get bpm(): number {
		return this.transport?.bpm.value ?? 120;
	}

	set bpm(value: number) {
		const t = this.transport;
		if (t) t.bpm.value = value;
	}

	get positionSeconds(): number {
		return this.transport?.seconds ?? 0;
	}

	play() {
		this.transport?.start();
	}

	pause() {
		this.transport?.pause();
	}

	stop() {
		this.transport?.stop();
	}

	seek(seconds: number) {
		const t = this.transport;
		if (t) t.seconds = seconds;
	}

	/** Load an audio file for a specific clip and return a handle. */
	async loadClip(clipId: string, url: string, volume = 0.8, pan = 0): Promise<void> {
		// Clean up existing player for this clip
		this.unloadClip(clipId);

		const channel = new Tone.Channel(Tone.gainToDb(volume), pan).toDestination();
		const player = new Tone.Player(url).connect(channel);
		await Tone.loaded();
		this.players.set(clipId, { player, channel });
	}

	/** Play a single clip immediately (for preview). */
	previewClip(clipId: string) {
		const handle = this.players.get(clipId);
		if (handle) handle.player.start();
	}

	/** Stop a clip preview. */
	stopClipPreview(clipId: string) {
		const handle = this.players.get(clipId);
		if (handle) handle.player.stop();
	}

	/** Update volume for a clip's channel (0–1). */
	setClipVolume(clipId: string, volume: number) {
		const handle = this.players.get(clipId);
		if (handle) handle.channel.volume.value = Tone.gainToDb(volume);
	}

	/** Update pan for a clip's channel (-1 to 1). */
	setClipPan(clipId: string, pan: number) {
		const handle = this.players.get(clipId);
		if (handle) handle.channel.pan.value = pan;
	}

	unloadClip(clipId: string) {
		const handle = this.players.get(clipId);
		if (handle) {
			handle.player.dispose();
			handle.channel.dispose();
			this.players.delete(clipId);
		}
	}

	dispose() {
		for (const [id] of this.players) {
			this.unloadClip(id);
		}
		this.transport?.stop();
	}
}

// Singleton — one engine per app session
export const audioEngine = new AudioEngine();
