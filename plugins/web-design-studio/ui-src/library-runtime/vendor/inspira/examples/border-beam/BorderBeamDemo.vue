<script setup lang="ts">
interface Props {
  size?: number;
  duration?: number;
  delay?: number;
  borderWidth?: number;
  anchor?: number;
  colorFrom?: string;
  colorTo?: string;
}

const props = withDefaults(defineProps<Props>(), {
  size: 250,
  duration: 12,
  delay: 9,
  borderWidth: 2,
  anchor: 90,
  colorFrom: "#ffaa40",
  colorTo: "#9c40ff",
});
</script>

<template>
  <ClientOnly>
    <div
      class="bg-background relative flex h-[500px] w-full flex-col items-center justify-center overflow-hidden rounded-lg border md:shadow-xl"
    >
      <span
        class="pointer-events-none bg-linear-to-b from-black to-gray-300/80 bg-clip-text text-center text-8xl leading-none font-semibold whitespace-pre-wrap text-transparent dark:from-white dark:to-slate-900/10"
      >
        Border Beam
      </span>
      <BorderBeam
        :size="props.size"
        :duration="props.duration"
        :delay="props.delay"
        :border-width="props.borderWidth"
        :anchor="props.anchor"
        :color-from="props.colorFrom"
        :color-to="props.colorTo"
      />
    </div>
  </ClientOnly>
</template>
