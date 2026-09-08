<script lang="ts" setup>
interface Props {
  morphTime?: number;
  coolDownTime?: number;
}

const props = withDefaults(defineProps<Props>(), {
  morphTime: 1.5,
  coolDownTime: 0.5,
});

const texts = [
  "Hello",
  "Morphing",
  "Text",
  "Animation",
  "Vue",
  "Component",
  "Smooth",
  "Transition",
  "Engaging",
];
</script>

<template>
  <ClientOnly>
    <MorphingText
      :texts="texts"
      :morph-time="props.morphTime"
      :cool-down-time="props.coolDownTime"
    />
  </ClientOnly>
</template>
