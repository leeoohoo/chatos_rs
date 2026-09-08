<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

interface Props {
  max?: number;
  min?: number;
  value?: number;
  autoPlay?: boolean;
  gaugePrimaryColor?: string;
  gaugeSecondaryColor?: string;
  circleStrokeWidth?: number;
  showPercentage?: boolean;
  duration?: number;
}

const props = withDefaults(defineProps<Props>(), {
  max: 100,
  min: 0,
  value: 70,
  autoPlay: true,
  gaugePrimaryColor: "rgb(79 70 229)",
  gaugeSecondaryColor: "rgba(0, 0, 0, 0.1)",
  circleStrokeWidth: 10,
  showPercentage: true,
  duration: 1,
});

const value = ref(props.value);

function handleIncrement(prev: number) {
  return prev >= props.max ? props.min : prev + 10;
}

let interval: ReturnType<typeof setInterval>;

onMounted(() => {
  if (!props.autoPlay) return;

  value.value = handleIncrement(value.value);
  interval = setInterval(() => {
    value.value = handleIncrement(value.value);
  }, 2000);
});

onBeforeUnmount(() => {
  if (interval) {
    clearInterval(interval);
  }
});
</script>

<template>
  <div class="flex items-center justify-center py-1">
    <AnimatedCircularProgressBar
      :max="props.max"
      :min="props.min"
      :value="value"
      :gauge-primary-color="props.gaugePrimaryColor"
      :gauge-secondary-color="props.gaugeSecondaryColor"
      :circle-stroke-width="props.circleStrokeWidth"
      :show-percentage="props.showPercentage"
      :duration="props.duration"
    />
  </div>
</template>
