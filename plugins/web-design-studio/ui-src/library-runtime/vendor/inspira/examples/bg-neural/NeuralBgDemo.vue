<script setup lang="ts">
interface Props {
  hue?: number;
  saturation?: number;
  chroma?: number;
}

const props = withDefaults(defineProps<Props>(), {
  hue: 200,
  saturation: 0.8,
  chroma: 0.6,
});
</script>

<template>
  <div class="relative flex h-96 min-h-96 w-full items-center justify-center text-xl">
    <NeuralBg
      :hue="props.hue"
      :saturation="props.saturation"
      :chroma="props.chroma"
    />
  </div>
</template>
