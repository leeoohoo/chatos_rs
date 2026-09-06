<script setup lang="ts">
interface Props {
  hue?: number;
  saturation?: number;
  brightness?: number;
  speed?: number;
  mouseSensitivity?: number;
  damping?: number;
  noise?: {
    opacity: number;
    scale: number;
  };
}

const props = defineProps<Props>();
</script>

<template>
  <div class="relative min-h-96 w-full">
    <StractiumBackground v-bind="props" />
  </div>
</template>
