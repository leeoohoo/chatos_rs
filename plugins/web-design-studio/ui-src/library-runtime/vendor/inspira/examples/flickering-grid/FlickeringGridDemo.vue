<script setup lang="ts">
interface Props {
  squareSize?: number;
  gridGap?: number;
  color?: string;
  maxOpacity?: number;
  flickerChance?: number;
}

const props = withDefaults(defineProps<Props>(), {
  squareSize: 4,
  gridGap: 6,
  color: "#60A5FA",
  maxOpacity: 0.5,
  flickerChance: 0.1,
});
</script>

<template>
  <div class="relative h-[600px] w-full overflow-hidden rounded-lg">
    <FlickeringGrid
      class="relative inset-0 z-0 mask-[radial-gradient(450px_circle_at_center,white,transparent)]"
      :square-size="props.squareSize"
      :grid-gap="props.gridGap"
      :color="props.color"
      :max-opacity="props.maxOpacity"
      :flicker-chance="props.flickerChance"
    />
  </div>
</template>
