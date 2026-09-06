<script setup lang="ts">
interface Props {
  text?: string;
  delay?: number;
  duration?: number;
  textEndColor?: string;
}

const props = withDefaults(defineProps<Props>(), {
  text: "f****ing great developer !",
  delay: 0,
  duration: 2000,
  textEndColor: "inherit",
});
</script>

<template>
  <div class="text-blance flex justify-center p-10">
    <h1 class="text-center text-4xl font-bold text-balance">
      You are a
      <TextHighlight
        class="bg-linear-to-r from-indigo-300 to-purple-300"
        :delay="props.delay"
        :duration="props.duration"
        :text-end-color="props.textEndColor"
      >
        {{ props.text }}
      </TextHighlight>
    </h1>
  </div>
</template>
