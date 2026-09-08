<script lang="ts" setup>
import type { VariantType } from "../../ui/image-trail-cursor/ImageTrailCursor.vue";

interface Props {
  variant?: VariantType;
}

const props = withDefaults(defineProps<Props>(), {
  variant: "type2",
});

const images = [
  "https://images.pexels.com/photos/799443/pexels-photo-799443.jpeg?auto=compress&cs=tinysrgb&w=1260&h=750&dpr=2",
  "https://images.pexels.com/photos/16245254/pexels-photo-16245254/free-photo-of-chatgpt-a-chatbot-for-your-website.jpeg?auto=compress&cs=tinysrgb&w=1260&h=750&dpr=2",
  "https://images.pexels.com/photos/1910236/pexels-photo-1910236.jpeg?auto=compress&cs=tinysrgb&w=1260&h=750&dpr=2",
  "https://images.pexels.com/photos/2832382/pexels-photo-2832382.jpeg?auto=compress&cs=tinysrgb&w=1260&h=750&dpr=2",
  "https://images.pexels.com/photos/2333293/pexels-photo-2333293.jpeg?auto=compress&cs=tinysrgb&w=1260&h=750&dpr=2",
  "https://images.pexels.com/photos/604684/pexels-photo-604684.jpeg?auto=compress&cs=tinysrgb&w=1260&h=750&dpr=2",
  "https://images.pexels.com/photos/3308588/pexels-photo-3308588.jpeg?auto=compress&cs=tinysrgb&w=1260&h=750&dpr=2",
];
</script>

<template>
  <div class="flex min-h-96 w-full flex-col gap-2">
    <div class="relative mt-4 flex h-96 w-full items-center justify-center rounded-lg p-4">
      <span class="absolute inset-0 flex items-center justify-center text-4xl">Hover Here</span>
      <ImageTrailCursor
        :key="props.variant"
        :images="images"
        :variant="props.variant"
      />
    </div>
  </div>
</template>
