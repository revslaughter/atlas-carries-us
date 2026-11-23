<!--
  @component
  The frame of every page: the title bar, then the page.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { data, children }: { data: { reader: string }; children: Snippet } = $props();
</script>

<header>The reading room, for {data.reader}</header>
<main>
  {@render children()}
</main>
