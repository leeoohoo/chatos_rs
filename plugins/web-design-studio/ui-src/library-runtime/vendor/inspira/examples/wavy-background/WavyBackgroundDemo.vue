<script setup lang="ts">
interface Props {
  waveWidth?: number;
  blur?: number;
  speed?: "slow" | "fast";
  waveOpacity?: number;
  backgroundFill?: string;
}

const props = withDefaults(defineProps<Props>(), {
  waveWidth: 50,
  blur: 10,
  speed: "fast",
  waveOpacity: 0.5,
  backgroundFill: "black",
});
</script>

<template>
  <WavyBackground
    class="mx-auto max-w-4xl pb-40"
    :wave-width="props.waveWidth"
    :blur="props.blur"
    :speed="props.speed"
    :wave-opacity="props.waveOpacity"
    :background-fill="props.backgroundFill"
  >
    <p class="inter-var text-center text-2xl font-bold text-white md:text-4xl lg:text-7xl">
      Hero waves are cool
    </p>
    <p class="inter-var mt-4 text-center text-base font-normal text-white md:text-lg">
      Leverage the power of canvas to create a beautiful hero section
    </p>
  </WavyBackground>
</template>
