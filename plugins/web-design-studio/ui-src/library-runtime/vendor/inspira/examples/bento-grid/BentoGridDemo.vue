<script lang="ts" setup>
const props = withDefaults(
  defineProps<{
    itemCount?: number;
    layout?: "featured" | "even";
    animatedHeaders?: boolean;
  }>(),
  {
    itemCount: 7,
    layout: "featured",
    animatedHeaders: true,
  },
);

const items = [
  {
    title: "The Dawn of Innovation",
    description: "Explore the birth of groundbreaking ideas and inventions.",
  },
  {
    title: "The Digital Revolution",
    description: "Dive into the transformative power of technology.",
  },
  {
    title: "The Art of Design",
    description: "Discover the beauty of thoughtful and experience design.",
  },
  {
    title: "The Power of Communication",
    description: "Understand the impact of effective communication in our lives.",
  },
  {
    title: "The Pursuit of Knowledge",
    description: "Join the quest for understanding and enlightenment.",
  },
  {
    title: "The Joy of Creation",
    description: "Experience the thrill of bringing ideas to life.",
  },
  {
    title: "The Spirit of Adventure",
    description: "Embark on exciting journeys and thrilling discoveries.",
  },
];

const visibleItems = computed(() => items.slice(0, props.itemCount));

function isWide(index: number) {
  return props.layout === "featured" && (index === 3 || index === 6);
}
</script>

<template>
  <BentoGrid class="mx-auto max-w-4xl">
    <BentoGridItem
      v-for="(item, index) in visibleItems"
      :key="index"
      :class="isWide(index) ? 'md:col-span-2' : ''"
    >
      <template #header>
        <div
          class="flex size-full space-x-4"
          :class="props.animatedHeaders ? 'animate-pulse' : ''"
        >
          <div class="flex size-full flex-1 rounded-md bg-zinc-800" />
        </div>
      </template>

      <template #title>
        <strong>{{ item.title }}</strong>
      </template>

      <template #icon />

      <template #description>
        <p>{{ item.description }}</p>
      </template>
    </BentoGridItem>
  </BentoGrid>
</template>
