<!--
  @component
  Tells a visitor what the reading room offers.
-->
<script lang="ts">
  let { data }: { data: { rooms: number } } = $props();
</script>

<p>The reading room has {data.rooms} rooms.</p>
