<script setup lang="ts">
interface Props {
  factor?: number;
  speed?: number;
  starColor?: string;
}

const props = withDefaults(defineProps<Props>(), {
  factor: 0.05,
  speed: 50,
  starColor: "#ffffff",
});
</script>

<template>
  <div
    class="relative flex h-96 w-full flex-col items-center justify-center gap-4 overflow-clip rounded-xl border border-zinc-800"
  >
    <StarsBackground
      :factor="props.factor"
      :speed="props.speed"
      :star-color="props.starColor"
    />
  </div>
</template>
