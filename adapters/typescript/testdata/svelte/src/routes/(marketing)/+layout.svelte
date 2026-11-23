<!--
  @component
  The frame of the marketing pages: a banner above the page.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { children }: { children: Snippet } = $props();
</script>

<div class="banner">Join the reading room</div>
{@render children()}
