<script setup lang="ts">
import { useColorMode } from "@vueuse/core";
import { computed } from "vue";

interface Props {
  quantity?: number;
  ease?: number;
  staticity?: number;
  color?: string;
}

const props = withDefaults(defineProps<Props>(), {
  quantity: 100,
  ease: 100,
  staticity: 10,
});

const isDark = computed(() => useColorMode().value === "dark");
const particleColor = computed(() => props.color || (isDark.value ? "#ffffff" : "#000000"));
</script>

<template>
  <div
    class="bg-background relative flex h-[500px] w-full flex-col items-center justify-center overflow-hidden rounded-lg md:shadow-xl"
  >
    <span
      class="pointer-events-none bg-linear-to-b from-black to-gray-300/80 bg-clip-text text-center text-8xl leading-none font-semibold whitespace-pre-wrap text-transparent dark:from-white dark:to-slate-900/10"
    >
      Particles
    </span>
    <ParticlesBg
      :key="`${isDark}-${props.quantity}`"
      class="absolute inset-0"
      :quantity="props.quantity"
      :ease="props.ease"
      :color="particleColor"
      :staticity="props.staticity"
      refresh
    />
  </div>
</template>
