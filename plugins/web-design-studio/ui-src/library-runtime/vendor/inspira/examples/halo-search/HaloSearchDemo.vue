<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    hint?: string;
    height?: "sm" | "md" | "lg";
    showHint?: boolean;
  }>(),
  {
    hint: "Hover the field, then type.",
    height: "md",
    showHint: true,
  },
);

const heightClass = computed(() => ({
  "h-80": props.height === "sm",
  "h-96": props.height === "md",
  "h-[28rem]": props.height === "lg",
}));
</script>

<template>
  <div
    class="relative z-0 flex w-full flex-col items-center justify-center"
    :class="heightClass"
  >
    <HaloSearch />

    <span
      v-if="props.showHint"
      class="text-muted mt-24 font-light"
    >
      {{ props.hint }}
    </span>
  </div>
</template>
