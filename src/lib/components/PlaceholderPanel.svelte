<script lang="ts">
	export let title: string;
	export let icon: string;
	export let description: string;
</script>

<div class="placeholder">
	<span class="icon">{icon}</span>
	<h2>{title}</h2>
	<p>{description}</p>
</div>

<style>
	.placeholder {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		color: var(--text-muted);
		padding: 60px;
		text-align: center;
	}

	.icon { font-size: 48px; opacity: 0.4; }

	h2 {
		font-size: 20px;
		font-weight: 600;
		color: var(--text-secondary);
	}

	p { font-size: 13px; max-width: 360px; line-height: 1.6; }
</style>
