<script setup lang="ts">
import { Motion } from "motion-v";

interface Props {
  radialGradient?: boolean;
  title?: string;
  subtitle?: string;
}

const props = withDefaults(defineProps<Props>(), {
  radialGradient: true,
  title: "Background lights are cool you know.",
  subtitle: "And this, is chemical burn.",
});
</script>

<template>
  <AuroraBackground :radial-gradient="props.radialGradient">
    <Motion
      as="div"
      :initial="{ opacity: 0, y: 40, filter: 'blur(10px)' }"
      :while-in-view="{
        opacity: 1,
        y: 0,
        filter: 'blur(0px)',
      }"
      :transition="{
        delay: 0.3,
        duration: 0.8,
        ease: 'easeInOut',
      }"
      class="relative flex flex-col items-center justify-center gap-4 px-4"
    >
      <div class="text-center text-3xl font-bold md:text-7xl dark:text-white">
        {{ props.title }}
      </div>
      <div class="py-4 text-base font-extralight md:text-4xl dark:text-neutral-200">
        {{ props.subtitle }}
      </div>
      <button
        class="w-fit rounded-full bg-black px-4 py-2 text-white dark:bg-white dark:text-black"
      >
        Burn it now
      </button>
    </Motion>
  </AuroraBackground>
</template>
