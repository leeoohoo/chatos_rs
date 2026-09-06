<script setup lang="ts">
interface Props {
  baseCircleSize?: number;
  baseCircleOpacity?: number;
  circleOpacityDowngradeRatio?: number;
  waveSpeed?: number;
  spaceBetweenCircle?: number;
  numberOfCircles?: number;
}

const props = withDefaults(defineProps<Props>(), {
  baseCircleSize: 210,
  baseCircleOpacity: 0.24,
  circleOpacityDowngradeRatio: 0.03,
  waveSpeed: 80,
  spaceBetweenCircle: 70,
  numberOfCircles: 7,
});
</script>

<template>
  <div
    class="relative flex h-[450px] w-full flex-col items-center justify-center overflow-hidden rounded-lg md:w-full lg:w-full"
  >
    <p
      class="z-10 text-center text-5xl font-medium tracking-tighter whitespace-pre-wrap text-black dark:text-white"
    >
      Ripple
    </p>
    <Ripple
      class="bg-white/5 mask-[linear-gradient(to_bottom,white,transparent)]"
      circle-class="border-[hsl(var(--primary))] bg-[#0000]/25 dark:bg-white/25 rounded-full"
      :base-circle-size="props.baseCircleSize"
      :base-circle-opacity="props.baseCircleOpacity"
      :circle-opacity-downgrade-ratio="props.circleOpacityDowngradeRatio"
      :wave-speed="props.waveSpeed"
      :space-between-circle="props.spaceBetweenCircle"
      :number-of-circles="props.numberOfCircles"
    />
  </div>
</template>
