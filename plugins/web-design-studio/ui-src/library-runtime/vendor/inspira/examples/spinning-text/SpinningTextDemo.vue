<script lang="ts" setup>
interface Props {
  text?: string;
  duration?: number;
  reverse?: boolean;
  radius?: number;
}

const props = withDefaults(defineProps<Props>(), {
  text: "learn more · earn more · grow more ·",
  duration: 10,
  reverse: false,
  radius: 5,
});
</script>

<template>
  <ClientOnly>
    <div class="flex min-h-[350px] w-full items-center justify-center">
      <SpinningText
        :text="props.text"
        :duration="props.duration"
        :reverse="props.reverse"
        :radius="props.radius"
      />
    </div>
  </ClientOnly>
</template>
