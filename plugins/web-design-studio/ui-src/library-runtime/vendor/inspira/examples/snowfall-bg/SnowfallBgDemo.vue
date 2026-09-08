<script setup lang="ts">
interface Props {
  color?: string;
  quantity?: number;
  speed?: number;
  minRadius?: number;
  maxRadius?: number;
}

const props = withDefaults(defineProps<Props>(), {
  color: "#ADD8E6",
  quantity: 100,
  speed: 0.5,
  minRadius: 0.2,
  maxRadius: 5,
});
</script>

<template>
  <div class="relative h-96 w-full">
    <SnowfallBg
      :color="props.color"
      class="absolute inset-0"
      :quantity="props.quantity"
      :min-radius="props.minRadius"
      :max-radius="props.maxRadius"
      :speed="props.speed"
    />
  </div>
</template>
