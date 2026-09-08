<script setup lang="ts">
interface Props {
  delay?: number;
  duration?: number;
  blur?: string;
  yOffset?: number;
}

const props = withDefaults(defineProps<Props>(), {
  delay: 0.2,
  duration: 0.75,
  blur: "20px",
  yOffset: 20,
});
</script>

<template>
  <ClientOnly>
    <BlurReveal
      :delay="props.delay"
      :duration="props.duration"
      :blur="props.blur"
      :y-offset="props.yOffset"
      class="p-8"
    >
      <h2 class="text-3xl font-bold tracking-tighter sm:text-5xl xl:text-6xl/none">Hey there 👋</h2>
      <span class="text-xl tracking-tighter text-pretty sm:text-3xl xl:text-4xl/none">
        How is it going?
      </span>
    </BlurReveal>
  </ClientOnly>
</template>
