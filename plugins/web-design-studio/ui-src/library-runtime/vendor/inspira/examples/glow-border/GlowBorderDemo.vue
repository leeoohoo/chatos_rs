<script setup lang="ts">
interface Props {
  borderRadius?: number;
  borderWidth?: number;
  duration?: number;
  color1?: string;
  color2?: string;
  color3?: string;
}

const props = withDefaults(defineProps<Props>(), {
  borderRadius: 10,
  borderWidth: 2,
  duration: 10,
  color1: "#A07CFE",
  color2: "#FE8FB5",
  color3: "#FFBE7B",
});
</script>

<template>
  <div class="relative h-[500px] w-full overflow-hidden">
    <GlowBorder
      :color="[props.color1, props.color2, props.color3]"
      :border-radius="props.borderRadius"
      :border-width="props.borderWidth"
      :duration="props.duration"
    />
    <div class="flex h-full w-full items-center justify-center">
      <span
        class="pointer-events-none bg-linear-to-b from-black to-gray-300/80 bg-clip-text text-center text-7xl leading-none font-semibold whitespace-pre-wrap text-transparent dark:from-white dark:to-zinc-700/75"
      >
        Glow Border
      </span>
    </div>
  </div>
</template>
