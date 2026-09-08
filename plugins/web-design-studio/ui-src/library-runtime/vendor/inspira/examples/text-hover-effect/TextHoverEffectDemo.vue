<script setup lang="ts">
interface Props {
  text?: string;
  strokeWidth?: number;
  duration?: number;
  opacity?: number;
}

const props = withDefaults(defineProps<Props>(), {
  text: "INSPIRA",
  strokeWidth: 0.75,
  duration: 200,
  opacity: 0.75,
});
</script>

<template>
  <ClientOnly>
    <div class="flex h-auto flex-col items-center justify-center gap-4 max-lg:w-full md:flex-1">
      <span class="text-xl font-medium">Hover below to reveal</span>
      <TextHoverEffect
        class="w-[90%] lg:min-h-64"
        :text="props.text"
        :stroke-width="props.strokeWidth"
        :duration="props.duration"
        :opacity="props.opacity"
      />
    </div>
  </ClientOnly>
</template>
