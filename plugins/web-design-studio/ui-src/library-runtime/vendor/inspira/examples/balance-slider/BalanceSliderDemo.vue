<script lang="ts" setup>
import { useColorMode } from "@vueuse/core";
import { computed } from "vue";

interface Props {
  initialValue?: number;
  leftColor?: string;
  rightColor?: string;
  minShiftLimit?: number;
  maxShiftLimit?: number;
  leftContent?: string;
  rightContent?: string;
  indicatorColor?: string;
  borderRadius?: number;
}

const props = withDefaults(defineProps<Props>(), {
  initialValue: 50,
  leftColor: "#e68a00",
  minShiftLimit: 40,
  maxShiftLimit: 68,
  leftContent: "COFFEE",
  rightContent: "MILK",
  borderRadius: 8,
});

const isDark = computed(() => useColorMode().value === "dark");
const rightColor = computed(() => props.rightColor || (isDark.value ? "#FFFFFF" : "#000000"));
const indicatorColor = computed(
  () => props.indicatorColor || (isDark.value ? "#FFFFFF" : "#000000"),
);
</script>

<template>
  <div class="w-full p-4">
    <BalanceSlider
      :initial-value="props.initialValue"
      :left-color="props.leftColor"
      :right-color="rightColor"
      :min-shift-limit="props.minShiftLimit"
      :max-shift-limit="props.maxShiftLimit"
      :left-content="props.leftContent"
      :right-content="props.rightContent"
      :indicator-color="indicatorColor"
      :border-radius="props.borderRadius"
    />
  </div>
</template>
