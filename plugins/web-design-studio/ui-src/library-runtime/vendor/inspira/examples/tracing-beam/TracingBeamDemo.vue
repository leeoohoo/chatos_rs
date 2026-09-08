<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    itemCount?: number;
    showImages?: boolean;
    compact?: boolean;
  }>(),
  {
    itemCount: 3,
    showImages: true,
    compact: false,
  },
);

const content = [
  {
    title: "Build with motion that explains itself",
    description: [
      "Tracing Beam keeps long-form content readable while giving readers a clear sense of progress.",
      "Use it for stories, timelines, release notes, or guided sections that benefit from scroll context.",
    ],
    badge: "Vue",
    image:
      "https://images.unsplash.com/photo-1464822759023-fed622ff2c3b?auto=format&fit=crop&q=80&w=3540&ixlib=rb-4.0.3&ixid=M3wxMjA3fDB8MHxwaG90by1wYWdlfHx8fGVufDB8fHx8fA%3D%3D",
  },
  {
    title: "Keep sections connected",
    description: [
      "The beam follows the reading path and creates a subtle relationship between independent content blocks.",
      "It stays decorative, so the page remains useful without the animation.",
    ],
    badge: "Nuxt",
    image:
      "https://images.unsplash.com/photo-1519681393784-d120267933ba?auto=format&fit=crop&q=80&w=3540&ixlib=rb-4.0.3&ixid=M3wxMjA3fDB8MHxwaG90by1wYWdlfHx8fGVufDB8fHx8fA%3D%3D",
  },
  {
    title: "Add motion without taking over",
    description: [
      "The effect is strongest when the content is calm: good spacing, readable type, and one visible path.",
    ],
    badge: "Inspira UI",
    image:
      "https://images.unsplash.com/photo-1469474968028-56623f02e42e?auto=format&fit=crop&q=80&w=3506&ixlib=rb-4.0.3&ixid=M3wxMjA3fDB8MHxwaG90by1wYWdlfHx8fGVufDB8fHx8fA%3D%3D",
  },
];

const visibleContent = computed(() => content.slice(0, props.itemCount));
</script>

<template>
  <div class="w-full items-center justify-center px-8">
    <TracingBeam class="px-6">
      <div class="relative mx-auto max-w-2xl pt-4 antialiased">
        <div
          v-for="(item, index) in visibleContent"
          :key="`content-${index}`"
          :class="props.compact ? 'mb-6' : 'mb-10'"
        >
          <div
            class="mb-4 w-fit rounded-full bg-black px-2 text-sm text-white dark:bg-white dark:text-black"
          >
            {{ item.badge }}
          </div>

          <p class="mb-4 text-xl">
            {{ item.title }}
          </p>

          <div class="prose prose-sm dark:prose-invert text-sm">
            <img
              v-if="props.showImages && item.image"
              :src="item.image"
              alt="blog thumbnail"
              class="mb-10 rounded-lg object-cover"
            />
            <div>
              <p
                v-for="(paragraph, idx) in item.description"
                :key="`desc-${idx}`"
              >
                {{ paragraph }}
              </p>
            </div>
          </div>
        </div>
      </div>
    </TracingBeam>
  </div>
</template>
