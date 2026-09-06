<script setup lang="ts">
interface Props {
  label?: string;
  placeholder?: string;
  wide?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  label: "Hover over below input",
  placeholder: "Hover over me",
  wide: false,
});
</script>

<template>
  <div class="flex h-56 w-full flex-col items-center justify-center gap-2">
    <label
      for="inputDemo"
      class="ml-4 w-full max-w-sm text-sm font-medium text-gray-700 dark:text-gray-200"
      :class="props.wide && 'max-w-xl'"
    >
      {{ props.label }}
    </label>
    <IInput
      id="inputDemo"
      :placeholder="props.placeholder"
      :container-class="props.wide ? 'w-full max-w-xl' : 'w-full max-w-sm'"
    />
  </div>
</template>
