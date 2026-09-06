<script setup lang="ts">
interface Props {
  text?: string;
  speed?: number;
  enableShadows?: boolean;
  enableOnHover?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  text: "Inspira UI",
  speed: 1,
  enableShadows: true,
  enableOnHover: false,
});
</script>

<template>
  <div class="flex w-full items-center justify-center rounded-lg p-4">
    <TextGlitch
      :speed="props.speed"
      :enable-shadows="props.enableShadows"
      :enable-on-hover="props.enableOnHover"
      :text="props.text"
    />
  </div>
</template>
