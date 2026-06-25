<script lang="ts">
	import { projectStore, type Clip, type Track } from '$lib/stores/projectStore';
	import { transportStore } from '$lib/stores/transportStore';
	import { convertFileSrc } from '@tauri-apps/api/core';

	let videoEl: HTMLVideoElement;
	let activeClipId: string | null = null;

	// Topmost video track wins when clips overlap — true multi-track compositing
	// only happens at export time via the melt render pipeline (see M4).
	$: videoTracks = $projectStore.tracks.filter((t): t is Track => t.type === 'video');
	$: bpm = $projectStore.bpm;
	$: activeClip = findActiveClip(videoTracks, $transportStore.positionBeats);

	function findActiveClip(tracks: Track[], positionBeats: number): Clip | null {
		for (const track of tracks) {
			if (track.muted) continue;
			for (const clip of track.clips) {
				if (positionBeats >= clip.startBeat && positionBeats < clip.startBeat + clip.durationBeats) {
					return clip;
				}
			}
		}
		return null;
	}

	$: if (activeClip?.id !== activeClipId) {
		activeClipId = activeClip?.id ?? null;
	}

	// Keep the <video> element's playback position in sync with the transport.
	$: if (videoEl && activeClip) {
		const clipStartSeconds = activeClip.startBeat * (60 / bpm);
		const offsetIntoClip = $transportStore.positionSeconds - clipStartSeconds;
		const sourceSeconds = (activeClip.videoInPoint ?? 0) + offsetIntoClip;
		if (Math.abs(videoEl.currentTime - sourceSeconds) > 0.15) {
			videoEl.currentTime = Math.max(0, sourceSeconds);
		}
		if ($transportStore.playing && videoEl.paused) {
			videoEl.play().catch(() => {});
		} else if (!$transportStore.playing && !videoEl.paused) {
			videoEl.pause();
		}
	}
</script>

<div class="video-preview">
	<div class="monitor">
		{#if activeClip?.filePath}
			{#key activeClip.id}
				<video bind:this={videoEl} src={convertFileSrc(activeClip.filePath)} muted={false}></video>
			{/key}
		{:else}
			<div class="empty-state">
				<p>No video clip under the playhead.</p>
				<p class="hint">Add a video track and clip from the Timeline panel.</p>
			</div>
		{/if}
	</div>
	{#if videoTracks.length > 1}
		<p class="limitation-note">
			Preview shows the topmost video track only. Overlapping tracks are composited together at export time.
		</p>
	{/if}
</div>

<style>
	.video-preview {
		flex: 1;
		display: flex;
		flex-direction: column;
		background: var(--bg-base);
		padding: 16px;
		overflow: hidden;
	}

	.monitor {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		background: #000;
		border-radius: 8px;
		overflow: hidden;
		border: 1px solid var(--border);
	}

	video {
		max-width: 100%;
		max-height: 100%;
	}

	.empty-state {
		text-align: center;
		color: var(--text-muted);
		font-size: 13px;
	}

	.hint {
		font-size: 11px;
		margin-top: 4px;
	}

	.limitation-note {
		font-size: 11px;
		color: var(--text-muted);
		text-align: center;
		margin-top: 8px;
	}
</style>
