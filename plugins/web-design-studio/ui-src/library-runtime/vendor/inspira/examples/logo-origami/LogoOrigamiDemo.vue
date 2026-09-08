<script setup lang="ts">
interface Props {
  duration?: number;
  delay?: number;
}

const props = withDefaults(defineProps<Props>(), {
  duration: 1.5,
  delay: 2.5,
});
</script>

<template>
  <div
    class="bg-background flex h-72 flex-col items-center justify-center gap-12 px-4 py-24 md:flex-row"
  >
    <LogoOrigami
      :duration="props.duration"
      :delay="props.delay"
    >
      <LogoOrigamiItem class="bg-orange-300 text-neutral-900">
        <Icon
          name="mdi:github"
          size="96"
        />
      </LogoOrigamiItem>
      <LogoOrigamiItem class="bg-green-300 text-neutral-900">
        <Icon
          name="logos:notion-icon"
          size="96"
        />
      </LogoOrigamiItem>
      <LogoOrigamiItem class="bg-cyan-300 text-neutral-900">
        <Icon
          name="logos:whatsapp-icon"
          size="96"
        />
      </LogoOrigamiItem>
      <LogoOrigamiItem class="bg-purple-300 text-black">
        <Icon
          name="logos:messenger"
          size="96"
        />
      </LogoOrigamiItem>
      <LogoOrigamiItem class="bg-white text-neutral-900">
        <Icon
          name="logos:google-drive"
          size="96"
        />
      </LogoOrigamiItem>
    </LogoOrigami>
  </div>
</template>
