<script lang="ts" setup>
interface Props {
  title?: string;
  visibleLogos?: number;
}

const props = withDefaults(defineProps<Props>(), {
  title: "Trusted by Companies like",
  visibleLogos: 8,
});

const logos = [
  {
    name: "Hume AI",
    path: "https://res.cloudinary.com/dfhp33ufc/image/upload/v1724242869/hume-ai_sxfeu8.svg",
  },
  {
    name: "Supabase",
    path: "https://res.cloudinary.com/dfhp33ufc/image/upload/v1724242819/supabase_bdbnvy.svg",
  },
  {
    name: "Apple",
    path: "https://res.cloudinary.com/dfhp33ufc/image/upload/v1724242912/apple_dark_wixhyc.svg",
  },
  {
    name: "Asana",
    path: "https://res.cloudinary.com/dfhp33ufc/image/upload/v1724243064/asana-logo_ulshgt.svg",
  },
  {
    name: "DigitalOcean",
    path: "https://res.cloudinary.com/dfhp33ufc/image/upload/v1724243142/digitalocean_werjgx.svg",
  },
  {
    name: "Github",
    path: "https://res.cloudinary.com/dfhp33ufc/image/upload/v1724243168/github-dark_u5eygu.svg",
  },
  {
    name: "Linear",
    path: "https://res.cloudinary.com/dfhp33ufc/image/upload/v1724243194/linear_dtq4zq.svg",
  },
  {
    name: "Loom",
    path: "https://res.cloudinary.com/dfhp33ufc/image/upload/v1724243196/loom_ehvtka.svg",
  },
];
</script>

<template>
  <div>
    <IconLogoCloud
      :logos="logos.slice(0, props.visibleLogos)"
      :title="props.title"
    />
  </div>
</template>
