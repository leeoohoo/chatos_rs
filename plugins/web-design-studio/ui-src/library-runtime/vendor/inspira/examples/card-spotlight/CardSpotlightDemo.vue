<script setup lang="ts">
import { useColorMode } from "@vueuse/core";
import { computed } from "vue";

interface Props {
  label?: string;
  gradientSize?: number;
  gradientColor?: string;
  gradientOpacity?: number;
}

const props = withDefaults(defineProps<Props>(), {
  label: "Card Spotlight",
  gradientSize: 200,
  gradientOpacity: 0.8,
});

const isDark = computed(() => useColorMode().value === "dark");
const spotlightColor = computed(
  () => props.gradientColor || (isDark.value ? "#363636" : "#C9C9C9"),
);
</script>

<template>
  <div class="flex h-[500px] w-full flex-col gap-4 lg:h-[250px] lg:flex-row">
    <CardSpotlight
      class="cursor-pointer flex-col items-center justify-center text-4xl whitespace-nowrap shadow-2xl"
      :gradient-size="props.gradientSize"
      :gradient-color="spotlightColor"
      :gradient-opacity="props.gradientOpacity"
    >
      {{ props.label }}
    </CardSpotlight>
  </div>
</template>
