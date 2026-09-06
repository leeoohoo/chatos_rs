<script setup lang="ts">
interface Props {
  hue?: number;
  saturation?: number;
  brightness?: number;
  speed?: number;
  mouseSensitivity?: number;
  damping?: number;
  frameRate?: number;
  pixelRatio?: number;
  noise?: {
    opacity: number;
    scale: number;
  };

  shaderCode: string;
}

const props = defineProps<Props>();
</script>

<template>
  <div class="flex h-96 w-full flex-col items-center justify-center gap-4">
    <ShaderToy
      class="h-full w-full"
      v-bind="props"
    />
  </div>
</template>
