<script setup lang="ts">
import { ref } from "vue";

interface Props {
  particleCount?: number;
  spread?: number;
  startVelocity?: number;
  scalar?: number;
}

const props = withDefaults(defineProps<Props>(), {
  particleCount: 120,
  spread: 70,
  startVelocity: 35,
  scalar: 1,
});

const confettiRef = ref<{ fire: (options?: Record<string, unknown>) => void } | null>(null);

function fireConfetti() {
  confettiRef.value?.fire({
    particleCount: props.particleCount,
    spread: props.spread,
    startVelocity: props.startVelocity,
    scalar: props.scalar,
  });
}
</script>

<template>
  <div
    class="bg-background relative flex h-[500px] w-full flex-col items-center justify-center overflow-hidden rounded-lg border md:shadow-xl"
  >
    <span
      class="pointer-events-none bg-linear-to-b from-black to-gray-300/80 bg-clip-text text-center text-8xl leading-none font-semibold whitespace-pre-wrap text-transparent dark:from-white dark:to-slate-900/10"
    >
      Confetti
    </span>

    <!-- Confetti component with ref -->
    <Confetti
      ref="confettiRef"
      class="absolute top-0 left-0 z-0 size-full"
    />
    <InteractiveHoverButton
      text="Fire confetti"
      class="relative z-10"
      @click="fireConfetti"
    />
  </div>
</template>
