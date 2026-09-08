<script setup lang="ts">
interface Props {
  sentence?: string;
  manualMode?: boolean;
  blurAmount?: number;
  borderColor?: string;
  animationDuration?: number;
  pauseBetweenAnimations?: number;
}

const props = withDefaults(defineProps<Props>(), {
  sentence: "Focus is the key",
  manualMode: false,
  blurAmount: 5,
  borderColor: "#00C16A",
  animationDuration: 1,
  pauseBetweenAnimations: 0.5,
});
</script>

<template>
  <div class="relative flex h-96 w-full flex-col items-center justify-center gap-24 overflow-clip">
    <Focus
      :sentence="props.sentence"
      :manual-mode="props.manualMode"
      :blur-amount="props.blurAmount"
      :border-color="props.borderColor"
      :animation-duration="props.animationDuration"
      :pause-between-animations="props.pauseBetweenAnimations"
    />
  </div>
</template>
