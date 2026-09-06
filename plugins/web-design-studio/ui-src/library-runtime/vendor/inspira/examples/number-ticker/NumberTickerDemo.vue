<script setup lang="ts">
interface Props {
  value?: number;
  duration?: number;
  delay?: number;
  decimalPlaces?: number;
  direction?: "up" | "down";
}

const props = withDefaults(defineProps<Props>(), {
  value: 100,
  duration: 1000,
  delay: 0,
  decimalPlaces: 0,
  direction: "up",
});
</script>

<template>
  <div class="flex min-h-64 items-center justify-center">
    <p class="text-8xl font-medium tracking-tighter whitespace-pre-wrap text-black dark:text-white">
      <NumberTicker
        :value="props.value"
        :duration="props.duration"
        :delay="props.delay"
        :decimal-places="props.decimalPlaces"
        :direction="props.direction"
      />
    </p>
  </div>
</template>
