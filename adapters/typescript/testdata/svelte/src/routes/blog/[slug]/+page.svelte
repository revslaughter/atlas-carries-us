<!--
  @component
  One blog post, with its like and comment forms.
-->
<script lang="ts">
  import Button from "#lib/Button.svelte";
  import { slugTitle } from "#lib/format.js";

  import type { PageProps } from "./$types";

  let { data }: PageProps = $props();
</script>

<h1>{slugTitle(data.slug)}</h1>
<form method="POST" action="?/like">
  <Button label="Like" primary />
</form>
<form method="POST" action="?/comment">
  <textarea name="text"></textarea>
  <Button label="Comment" />
</form>
