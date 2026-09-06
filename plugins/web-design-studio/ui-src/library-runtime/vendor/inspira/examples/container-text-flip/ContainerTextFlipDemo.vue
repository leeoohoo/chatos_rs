<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    words?: string;
    interval?: number;
    animationDuration?: number;
  }>(),
  {
    words: "better, modern, beautiful, awesome",
    interval: 3000,
    animationDuration: 700,
  },
);

const words = computed(() =>
  props.words
    .split(",")
    .map((word) => word.trim())
    .filter(Boolean),
);
</script>

<template>
  <div class="flex w-full items-center justify-center">
    <ContainerTextFlip
      :words="words"
      :interval="props.interval"
      :animation-duration="props.animationDuration"
    />
  </div>
</template>
