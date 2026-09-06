<script setup lang="ts">
import { Motion } from "motion-v";

interface Props {
  text?: string;
  highlight?: string;
  duration?: number;
}

const props = withDefaults(defineProps<Props>(), {
  text: "The best",
  highlight: "components",
  duration: 0.5,
});
</script>

<template>
  <div class="relative flex h-screen w-full items-center justify-center overflow-hidden bg-black">
    <Motion
      as="img"
      src="https://assets.aceternity.com/linear-demo.webp"
      class="pointer-events-none absolute inset-0 size-full mask-[radial-gradient(circle,transparent,black_80%)] object-cover"
      :initial="{ opacity: 0 }"
      :while-in-view="{
        opacity: 0.5,
      }"
      :transition="{
        duration: 1,
      }"
    />
    <div>
      <h1
        class="relative z-2 text-center font-sans text-2xl font-bold text-white md:text-5xl lg:text-7xl"
      >
        {{ props.text }}
        <ColourfulText
          :text="props.highlight"
          :duration="props.duration"
        />
        <br />
        you will ever find
      </h1>
    </div>
  </div>
</template>
