<script lang="ts" setup>
interface Props {
  title?: string;
  duration?: number;
}

const props = withDefaults(defineProps<Props>(), {
  title: "Testimonials",
  duration: 6000,
});

const testimonials = [
  {
    quote:
      "The component structure is clear, flexible, and surprisingly elegant. It just works—no shadow clones required.",
    author: "Naruto Uzumaki",
    role: "Lead Frontend Ninja",
    company: "Hidden Leaf Labs",
  },
  {
    quote:
      "Thoughtful motion, precise timing, and no unnecessary noise. Design should feel effortless—this does.",
    author: "Itachi Uchiha",
    role: "Architect of Interfaces",
    company: "Akatsuki Systems",
  },
  {
    quote: "Clean visuals, smooth transitions, and zero friction. Honestly? It feels limitless.",
    author: "Gojo Satoru",
    role: "UI Performance Engineer",
    company: "Jujutsu Tech",
  },
  {
    quote:
      "Every detail feels intentional. The balance between motion and restraint is… impressive.",
    author: "Jin Sunwoo",
    role: "Product Strategist",
    company: "Shadow Guild",
  },
];
</script>
<template>
  <DesignTestimonials
    :testimonials
    :title="props.title"
    :duration="props.duration"
  />
</template>
