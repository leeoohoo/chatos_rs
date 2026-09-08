<script setup lang="ts">
import { ref } from "vue";

interface Props {
  title?: string;
  firstPlaceholder?: string;
}

const props = withDefaults(defineProps<Props>(), {
  title: "Ask Anything",
  firstPlaceholder: "How do I ship this faster?",
});

const placeholders = [
  props.firstPlaceholder,
  "What does 'undefined' even mean?",
  "How to center a div (for real this time)",
  "Am I smarter than a compiler?",
  "Do loops ever get dizzy?",
];
const text = ref("");
</script>

<template>
  <div class="flex h-160 flex-col items-center justify-center px-4">
    <h2 class="mb-10 text-center text-xl text-black sm:mb-20 sm:text-5xl dark:text-white">
      {{ props.title }}
    </h2>
    <VanishingInput
      v-model="text"
      :placeholders="placeholders"
    />
  </div>
</template>
