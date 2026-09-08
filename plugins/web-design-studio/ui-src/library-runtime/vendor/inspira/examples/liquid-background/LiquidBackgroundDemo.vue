<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    label?: string;
    height?: "sm" | "md" | "lg";
    rounded?: "none" | "lg" | "xl";
    showLabel?: boolean;
  }>(),
  {
    label: "Liquid background",
    height: "md",
    rounded: "lg",
    showLabel: true,
  },
);

const wrapperClass = computed(() => [
  {
    "h-72": props.height === "sm",
    "h-96": props.height === "md",
    "h-[32rem]": props.height === "lg",
    "rounded-none": props.rounded === "none",
    "rounded-lg": props.rounded === "lg",
    "rounded-xl": props.rounded === "xl",
  },
]);
</script>

<template>
  <div
    class="relative w-full overflow-hidden"
    :class="wrapperClass"
  >
    <LiquidBackground />
    <div
      v-if="props.showLabel"
      class="pointer-events-none absolute inset-0 grid place-items-center text-center text-3xl font-semibold text-white md:text-5xl"
    >
      {{ props.label }}
    </div>
  </div>
</template>
