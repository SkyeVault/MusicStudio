<script lang="ts">
	import { page } from '$app/stores';
</script>

<div style="color:#f88;background:#111;padding:24px;font-family:monospace;font-size:13px;white-space:pre-wrap;height:100vh;overflow:auto">
SVELTEKIT ERROR — {$page.status}

{$page.error?.message}

{$page.error?.stack ?? ''}
</div>
