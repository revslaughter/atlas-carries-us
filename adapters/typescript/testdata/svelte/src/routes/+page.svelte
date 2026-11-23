<!--
  @component
  The home page: a counter and a greeting.
-->
<script lang="ts">
  import Counter from "#lib/Counter.svelte";
  import Legacy from "#lib/Legacy.svelte";
  import { plural } from "#lib/format.js";

  /** Records a wave in the console. */
  function onWave(name: string) {
    console.info(plural(1, "wave"), name);
  }
</script>

<Counter start={1} />
<Legacy name="Ada" on:wave={e => onWave(e.detail.name)}>
  <p>Welcome back.</p>
</Legacy>
