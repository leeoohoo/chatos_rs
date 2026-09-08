<script setup lang="ts">
interface Props {
  duration?: number;
  color?: "zinc" | "blue" | "green" | "orange" | "red" | "purple";
  isStatic?: boolean;
  size?: "sm" | "md" | "lg";
  radius?: "sm" | "md" | "lg" | "xl";
  shadowSize?: "sm" | "md" | "lg" | "xl";
}

const props = withDefaults(defineProps<Props>(), {
  duration: 1000,
  color: "zinc",
  isStatic: false,
  size: "md",
  radius: "md",
  shadowSize: "lg",
});
</script>

<template>
  <div class="grid place-content-center p-10">
    <Book
      :duration="props.duration"
      :color="props.color"
      :is-static="props.isStatic"
      :size="props.size"
      :radius="props.radius"
      :shadow-size="props.shadowSize"
    >
      <BookHeader>
        <Icon
          name="heroicons:book-open-solid"
          size="24"
        />
      </BookHeader>
      <BookTitle>
        <h1>The Book</h1>
      </BookTitle>
      <BookDescription>
        <p>Hover me to animate!</p>
      </BookDescription>
    </Book>
  </div>
</template>
