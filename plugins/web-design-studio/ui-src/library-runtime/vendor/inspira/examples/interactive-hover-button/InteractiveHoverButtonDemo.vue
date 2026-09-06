<script lang="ts" setup>
interface Props {
  text?: string;
  wide?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  text: "Button",
  wide: false,
});
</script>

<template>
  <div class="flex w-full items-center justify-center p-4">
    <InteractiveHoverButton
      :text="props.text"
      :class="props.wide ? 'min-w-56' : ''"
    />
  </div>
</template>
