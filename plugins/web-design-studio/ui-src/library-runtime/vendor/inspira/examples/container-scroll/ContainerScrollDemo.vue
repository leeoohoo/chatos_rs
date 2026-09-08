<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    eyebrow?: string;
    headline?: string;
    highlight?: string;
    imagePosition?: "top" | "center" | "bottom";
  }>(),
  {
    eyebrow: "Scroll driven",
    headline: "Unleash the power of",
    highlight: "Scroll Animations",
    imagePosition: "top",
  },
);

const imagePositionClass = computed(() => ({
  "object-top": props.imagePosition === "top",
  "object-center": props.imagePosition === "center",
  "object-bottom": props.imagePosition === "bottom",
}));
</script>

<template>
  <div class="flex flex-col overflow-hidden">
    <ContainerScroll>
      <template #title>
        <h1 class="text-4xl font-semibold text-black dark:text-white">
          <span class="text-primary mb-4 block text-sm font-medium tracking-[0.24em] uppercase">
            {{ props.eyebrow }}
          </span>
          {{ props.headline }} <br />
          <span class="mt-1 text-4xl leading-none font-bold md:text-[6rem]">
            {{ props.highlight }}
          </span>
        </h1>
      </template>
      <template #card>
        <img
          src="https://cdn.inspira-ui.com/images/linear.webp"
          class="mx-auto h-full rounded-2xl object-cover"
          :class="imagePositionClass"
          alt="hero"
          height="720"
          width="1400"
        />
      </template>
    </ContainerScroll>
  </div>
</template>
