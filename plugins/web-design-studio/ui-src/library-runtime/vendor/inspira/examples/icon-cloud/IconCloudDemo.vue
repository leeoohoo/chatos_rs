<script setup lang="ts">
interface Props {
  iconCount?: number;
}

const props = withDefaults(defineProps<Props>(), {
  iconCount: 30,
});

const slugs = [
  "typescript",
  "javascript",
  "dart",
  "java",
  "react",
  "flutter",
  "android",
  "html5",
  "css3",
  "nodedotjs",
  "express",
  "nextdotjs",
  "prisma",
  "amazonaws",
  "postgresql",
  "firebase",
  "nginx",
  "vercel",
  "testinglibrary",
  "jest",
  "cypress",
  "docker",
  "git",
  "jira",
  "github",
  "gitlab",
  "visualstudiocode",
  "androidstudio",
  "sonarqube",
  "figma",
];

const imageUrls = computed(() =>
  slugs.slice(0, props.iconCount).map((slug) => `https://cdn.simpleicons.org/${slug}/${slug}`),
);
</script>

<template>
  <div class="grid place-content-center p-6">
    <IconCloud :images="imageUrls" />
  </div>
</template>
