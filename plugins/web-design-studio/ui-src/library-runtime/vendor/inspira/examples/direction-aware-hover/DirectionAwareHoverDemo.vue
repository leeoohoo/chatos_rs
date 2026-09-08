<script setup lang="ts">
interface Card {
  title: string;
  description: string;
  imageUrl: string;
}

const props = withDefaults(
  defineProps<{
    visibleCards?: number;
    shape?: "soft" | "sharp" | "round";
    showDescriptions?: boolean;
  }>(),
  {
    visibleCards: 4,
    shape: "soft",
    showDescriptions: true,
  },
);

const cards: Card[] = [
  {
    title: "Landscape",
    description: "Soft directional reveal.",
    imageUrl:
      "https://images.unsplash.com/photo-1728755833852-2c138c84cfb1?q=80&w=2672&auto=format&fit=crop",
  },
  {
    title: "City",
    description: "Overlay follows pointer entry.",
    imageUrl:
      "https://images.unsplash.com/photo-1522075469751-3a6694fb2f61?auto=format&fit=crop&w=900&q=80",
  },
  {
    title: "Dining",
    description: "Works with extra content.",
    imageUrl:
      "https://images.unsplash.com/photo-1664710476481-1213c456c56c?q=80&w=2672&auto=format&fit=crop",
  },
  {
    title: "Mountain",
    description: "Clean image-first cards.",
    imageUrl:
      "https://images.unsplash.com/photo-1506905925346-21bda4d32df4?q=80&w=2670&auto=format&fit=crop",
  },
  {
    title: "Studio",
    description: "Rounded art direction.",
    imageUrl:
      "https://images.unsplash.com/photo-1549298916-b41d501d3772?q=80&w=2624&auto=format&fit=crop",
  },
  {
    title: "Forest",
    description: "Subtle motion on hover.",
    imageUrl:
      "https://images.unsplash.com/photo-1441974231531-c6227db76b6e?q=80&w=2671&auto=format&fit=crop",
  },
];

const visibleCards = computed(() => cards.slice(0, props.visibleCards));

const shapeClass = computed(() => ({
  "rounded-lg": props.shape === "sharp",
  "rounded-2xl": props.shape === "soft",
  "rounded-full": props.shape === "round",
}));

const childrenClass = computed(() => [
  "bg-linear-to-t from-black/80 to-transparent p-4",
  shapeClass.value,
]);
</script>

<template>
  <div class="mx-auto grid w-full max-w-5xl grid-cols-1 gap-6 p-4 sm:grid-cols-2">
    <div
      v-for="card in visibleCards"
      :key="card.title"
      class="flex justify-center"
    >
      <DirectionAwareHover
        :image-url="card.imageUrl"
        class="shadow-lg ring-1 ring-gray-200 dark:ring-gray-800"
        :class="shapeClass"
        image-class="transition-transform duration-500"
        :children-class="childrenClass"
      >
        <h2 class="text-lg font-semibold sm:text-xl">{{ card.title }}</h2>
        <p
          v-if="props.showDescriptions"
          class="mt-1 text-sm opacity-90 sm:mt-2 sm:text-base"
        >
          {{ card.description }}
        </p>
      </DirectionAwareHover>
    </div>
  </div>
</template>
