// @ts-nocheck
"use client"

import {
  Button,
  Combobox,
  Field,
  Portal,
  Stack,
  useComboboxContext,
  useFilter,
  useListCollection,
} from "@chakra-ui/react"

// This is a hidden input that is used to store the value of the combobox
const ComboboxHiddenInput = (props: React.ComponentProps<"input">) => {
  const combobox = useComboboxContext()
  return <input type="hidden" value={combobox.value[0]} readOnly {...props} />
}

export const ComboboxWithFormSubmit = () => {
  const { contains } = useFilter({ sensitivity: "base" })

  const { collection, filter } = useListCollection({
    initialItems: countries,
    itemToString: (item) => item.country,
    itemToValue: (item) => item.code,
    filter: contains,
  })

  const handleSubmit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const formData = new FormData(event.currentTarget)
    const country = formData.get("country")
    console.log("Form submitted with country code:", country)
    alert(`Selected country code: ${country}`)
  }

  const handleInputChange = (details: Combobox.InputValueChangeDetails) => {
    filter(details.inputValue)
  }

  return (
    <form onSubmit={handleSubmit}>
      <Stack gap="4" align="flex-start">
        <Field.Root width="320px">
          <Field.Label>Country</Field.Label>
          <Combobox.Root
            collection={collection}
            onInputValueChange={handleInputChange}
          >
            <Combobox.Control>
              <Combobox.Input placeholder="Search countries (e.g. United States)" />
              <Combobox.IndicatorGroup>
                <Combobox.ClearTrigger />
                <Combobox.Trigger />
              </Combobox.IndicatorGroup>
            </Combobox.Control>

            <ComboboxHiddenInput name="country" />

            <Portal>
              <Combobox.Positioner>
                <Combobox.Content>
                  <Combobox.Empty>No countries found</Combobox.Empty>
                  {collection.items.map((item) => (
                    <Combobox.Item key={item.code} item={item}>
                      {item.flag} {item.country}
                      <Combobox.ItemIndicator />
                    </Combobox.Item>
                  ))}
                </Combobox.Content>
              </Combobox.Positioner>
            </Portal>
          </Combobox.Root>
          <Field.HelperText>
            The form will submit the country code (e.g. "US"), not the display
            name
          </Field.HelperText>
        </Field.Root>

        <Button size="sm" type="submit">
          Submit
        </Button>
      </Stack>
    </form>
  )
}

const countries = [
  { country: "United States", code: "US", flag: "🇺🇸" },
  { country: "Canada", code: "CA", flag: "🇨🇦" },
  { country: "Australia", code: "AU", flag: "🇦🇺" },
  { country: "United Kingdom", code: "GB", flag: "🇬🇧" },
  { country: "New Zealand", code: "NZ", flag: "🇳🇿" },
  { country: "South Africa", code: "ZA", flag: "🇿🇦" },
  { country: "India", code: "IN", flag: "🇮🇳" },
  { country: "China", code: "CN", flag: "🇨🇳" },
  { country: "Japan", code: "JP", flag: "🇯🇵" },
  { country: "Korea", code: "KR", flag: "🇰🇷" },
  { country: "Vietnam", code: "VN", flag: "🇻🇳" },
  { country: "Thailand", code: "TH", flag: "🇹🇭" },
  { country: "Malaysia", code: "MY", flag: "🇲🇾" },
  { country: "Indonesia", code: "ID", flag: "🇮🇩" },
  { country: "Philippines", code: "PH", flag: "🇵🇭" },
  { country: "Singapore", code: "SG", flag: "🇸🇬" },
  { country: "Hong Kong", code: "HK", flag: "🇭🇰" },
  { country: "Macau", code: "MO", flag: "🇲🇴" },
  { country: "Taiwan", code: "TW", flag: "🇹🇼" },
]
