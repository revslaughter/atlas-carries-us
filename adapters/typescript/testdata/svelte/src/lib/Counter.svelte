<!--
  @component
  Counts up from a start value, and says when the count passes a limit.
-->
<script lang="ts">
  import Button from "./Button.svelte";
  import { createCounter } from "./counter.svelte.js";
  import { plural } from "./format.js";

  let {
    start = 0,
    limit = 10,
  }: {
    /** Where the count starts. */
    start?: number;
    /** The count above which the counter says "enough". */
    limit?: number;
  } = $props();

  const counter = createCounter(start);
  /** The count, doubled. */
  let doubled = $derived(counter.count * 2);
  /** Whether the count has passed the limit. */
  let over = $derived(counter.count > limit);

  $effect(() => {
    if (over) console.info(plural(counter.count, "press"));
  });
</script>

<p>{plural(counter.count, "press")} ({doubled})</p>
<Button label="Add one" onclick={() => counter.increment()} />
{#if over}<p>That is enough.</p>{/if}
