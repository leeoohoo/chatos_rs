<script setup lang="ts">
interface Props {
  text?: string;
  duration?: number;
  animateOnLoad?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  text: "Hyper Text",
  duration: 800,
  animateOnLoad: true,
});
</script>

<template>
  <div class="flex flex-col items-center justify-center space-y-4">
    <HyperText
      :text="props.text"
      class="text-4xl font-bold"
      :duration="props.duration"
      :animate-on-load="props.animateOnLoad"
    />
  </div>
</template>
