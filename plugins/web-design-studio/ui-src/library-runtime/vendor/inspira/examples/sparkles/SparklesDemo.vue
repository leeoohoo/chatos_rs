<script setup lang="ts">
import { useColorMode } from "@vueuse/core";
import { computed } from "vue";

interface Props {
  text?: string;
  minSize?: number;
  maxSize?: number;
  particleDensity?: number;
  particleColor?: string;
}

const props = withDefaults(defineProps<Props>(), {
  text: "Inspira UI",
  minSize: 0.4,
  maxSize: 1.4,
  particleDensity: 1200,
});

const modeParticleColor = computed(() => (useColorMode().value === "dark" ? "#FFFFFF" : "#000000"));
const particlesColor = computed(() => props.particleColor || modeParticleColor.value);
</script>
<template>
  <div
    class="flex h-160 w-full flex-col items-center justify-center overflow-hidden rounded-md bg-white dark:bg-black"
  >
    <h1
      class="relative z-20 text-center text-3xl font-bold text-black md:text-7xl lg:text-9xl dark:text-white"
    >
      {{ props.text }}
    </h1>
    <div class="relative h-40 w-160">
      <div
        class="absolute inset-x-20 top-0 h-[2px] w-3/4 bg-linear-to-r from-transparent via-indigo-500 to-transparent blur-sm"
      />
      <div
        class="absolute inset-x-20 top-0 h-px w-3/4 bg-linear-to-r from-transparent via-indigo-500 to-transparent"
      />
      <div
        class="absolute inset-x-60 top-0 h-[5px] w-1/4 bg-linear-to-r from-transparent via-sky-500 to-transparent blur-sm"
      />
      <div
        class="absolute inset-x-60 top-0 h-px w-1/4 bg-linear-to-r from-transparent via-sky-500 to-transparent"
      />

      <Sparkles
        background="transparent"
        :min-size="props.minSize"
        :max-size="props.maxSize"
        :particle-density="props.particleDensity"
        class="size-full"
        :particle-color="particlesColor"
      />

      <div
        class="absolute inset-0 size-full bg-white mask-[radial-gradient(350px_200px_at_top,transparent_20%,white)] dark:bg-black"
      />
    </div>
  </div>
</template>
