<!--
  @component
  A button that names its action. Every form in the reading room uses it.

  ```svelte
  <Button label="Save" primary onclick={save} />
  ```
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { shout } from "./format.js";

  interface Props {
    /** The text on the button. */
    label: string;
    /** Marks the main action of a page. */
    primary?: boolean;
    /** Runs when a reader presses the button. */
    onclick?: (event: MouseEvent) => void;
    /** Content after the label. */
    children?: Snippet;
  }

  let { label, primary = false, onclick, children }: Props = $props();

  /** Counts the presses, for the badge. */
  let presses = $state(0);

  /** Records a press, then runs the reader's handler. */
  function press(event: MouseEvent) {
    presses += 1;
    onclick?.(event);
  }
</script>

{#snippet badge(count: number)}
  <span class="badge">{shout(String(count))}</span>
{/snippet}

<button class:primary onclick={press}>
  {label}
  {#if presses > 0}
    {@render badge(presses)}
  {/if}
  {@render children?.()}
</button>
