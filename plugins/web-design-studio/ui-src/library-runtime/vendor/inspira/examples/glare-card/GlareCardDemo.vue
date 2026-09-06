<script setup lang="ts">
interface Props {
  label?: string;
  tone?: "dark" | "slate" | "warm";
}

const props = withDefaults(defineProps<Props>(), {
  label: "Inspira UI",
  tone: "dark",
});

const toneClass = computed(() => {
  if (props.tone === "slate") return "bg-slate-950";
  if (props.tone === "warm") return "bg-stone-950";
  return "bg-neutral-950";
});
</script>

<template>
  <div class="flex items-center justify-center">
    <GlareCard
      class="flex flex-col items-center justify-center"
      :class="toneClass"
    >
      <img src="https://cdn.inspira-ui.com../../assets/logo-dark.svg" />
      <p class="mt-4 text-xl font-bold text-white">{{ props.label }}</p>
    </GlareCard>
  </div>
</template>
