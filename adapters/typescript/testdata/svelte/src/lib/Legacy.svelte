<!--
  @component
  Greets a reader, in the older component syntax: `export let` props, an
  event dispatcher and a slot.
-->
<script context="module" lang="ts">
  /** The greeting when the page names no reader. */
  export const DEFAULT_NAME = "reader";

  /** Says whether a name needs the default greeting. */
  export function isBlank(name: string): boolean {
    return name.trim() === "";
  }
</script>

<script lang="ts">
  import { createEventDispatcher } from "svelte";

  import { shout } from "./format.js";

  /** The reader's name. */
  export let name = DEFAULT_NAME;
  /** Says the greeting louder. */
  export let loud = false;

  const dispatch = createEventDispatcher<{
    /** Fires when the reader waves back. */
    wave: { name: string };
  }>();

  /** Sends the wave event with the reader's name. */
  function waveBack() {
    dispatch("wave", { name });
  }
</script>

<p>Hello, {loud ? shout(name) : isBlank(name) ? DEFAULT_NAME : name}!</p>
<button on:click={waveBack}>Wave</button>
<slot />
