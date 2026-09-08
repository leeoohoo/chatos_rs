// @ts-nocheck
"use client"

import { Portal, Select, createListCollection } from "@chakra-ui/react"
import { groupBy } from "es-toolkit"

export const SelectWithCountry = () => {
  return (
    <Select.Root
      collection={countries}
      size="sm"
      width="320px"
      defaultValue={["NG"]}
    >
      <Select.HiddenSelect />
      <Select.Label>Select country</Select.Label>
      <Select.Control>
        <Select.Trigger>
          <Select.ValueText placeholder="-" />
        </Select.Trigger>
        <Select.IndicatorGroup>
          <Select.Indicator />
        </Select.IndicatorGroup>
      </Select.Control>
      <Portal>
        <Select.Positioner>
          <Select.Content>
            {continents.map(([continent, items]) => (
              <Select.ItemGroup key={continent}>
                <Select.ItemGroupLabel>{continent}</Select.ItemGroupLabel>
                {items.map((item) => (
                  <Select.Item item={item} key={item.value}>
                    {countries.stringifyItem(item)}
                    <Select.ItemIndicator />
                  </Select.Item>
                ))}
              </Select.ItemGroup>
            ))}
          </Select.Content>
        </Select.Positioner>
      </Portal>
    </Select.Root>
  )
}

const countries = createListCollection({
  items: [
    { value: "US", label: "United States", flag: "🇺🇸", continent: "America" },
    { value: "CA", label: "Canada", flag: "🇨🇦", continent: "America" },
    { value: "MX", label: "Mexico", flag: "🇲🇽", continent: "America" },
    { value: "BR", label: "Brazil", flag: "🇧🇷", continent: "America" },
    { value: "ZA", label: "South Africa", flag: "🇿🇦", continent: "Africa" },
    { value: "NG", label: "Nigeria", flag: "🇳🇬", continent: "Africa" },
    { value: "MA", label: "Morocco", flag: "🇲🇦", continent: "Africa" },
    { value: "EG", label: "Egypt", flag: "🇪🇬", continent: "Africa" },
    { value: "CN", label: "China", flag: "🇨🇳", continent: "Asia" },
    { value: "JP", label: "Japan", flag: "🇯🇵", continent: "Asia" },
    { value: "IN", label: "India", flag: "🇮🇳", continent: "Asia" },
    { value: "KR", label: "South Korea", flag: "🇰🇷", continent: "Asia" },
    { value: "GB", label: "United Kingdom", flag: "🇬🇧", continent: "Europe" },
    { value: "FR", label: "France", flag: "🇫🇷", continent: "Europe" },
    { value: "DE", label: "Germany", flag: "🇩🇪", continent: "Europe" },
    { value: "IT", label: "Italy", flag: "🇮🇹", continent: "Europe" },
    { value: "ES", label: "Spain", flag: "🇪🇸", continent: "Europe" },
    { value: "AU", label: "Australia", flag: "🇦🇺", continent: "Oceania" },
    { value: "NZ", label: "New Zealand", flag: "🇳🇿", continent: "Oceania" },
    { value: "FJ", label: "Fiji", flag: "🇫🇯", continent: "Oceania" },
  ],
  itemToString: (item) => `${item.flag} ${item.label}`,
  itemToValue: (item) => item.value,
})

const continents = Object.entries(
  groupBy(countries.items, (item) => item.continent),
)
