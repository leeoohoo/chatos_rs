<script setup lang="ts">
import { useColorMode } from "@vueuse/core";
import { computed } from "vue";

interface Props {
  color?: string;
  count?: number;
}

const props = withDefaults(defineProps<Props>(), {
  count: 200,
});

const isDark = computed(() => useColorMode().value === "dark");
const starColor = computed(() => props.color || (isDark.value ? "#ffffff" : "#555555"));
</script>

<template>
  <div class="relative flex h-96 flex-col items-center justify-center">
    <FallingStarsBg
      class="bg-white dark:bg-black"
      :color="starColor"
      :count="props.count"
    />
    <div class="z-1 flex items-center">
      <span class="text-6xl font-bold text-black dark:text-white">Inspira UI</span>
    </div>
  </div>
</template>
