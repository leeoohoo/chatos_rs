// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo, useState, type UIEvent } from 'react';
import { useInfiniteQuery } from '@tanstack/react-query';
import { Select, type SelectProps } from 'antd';

import { api } from '../api/client';
import type { UserOptionRecord } from '../types';

const USER_OPTION_PAGE_SIZE = 20;

type UserOptionSelectProps = Omit<
  SelectProps<string>,
  'filterOption' | 'loading' | 'onPopupScroll' | 'onSearch' | 'options' | 'showSearch'
> & {
  seedOptions?: UserOptionRecord[];
  fixedOptions?: Array<{ label: string; value: string }>;
};

export function UserOptionSelect({
  seedOptions = [],
  fixedOptions = [],
  disabled,
  onChange,
  onOpenChange,
  value,
  ...selectProps
}: UserOptionSelectProps) {
  const [open, setOpen] = useState(false);
  const [searchInput, setSearchInput] = useState('');
  const [search, setSearch] = useState('');
  const [selectedUser, setSelectedUser] = useState<UserOptionRecord>();

  useEffect(() => {
    const timer = window.setTimeout(() => setSearch(searchInput.trim()), 250);
    return () => window.clearTimeout(timer);
  }, [searchInput]);

  const usersQuery = useInfiniteQuery({
    queryKey: ['user-service', 'user-options', search],
    queryFn: ({ pageParam }) => api.listUserOptions(USER_OPTION_PAGE_SIZE, pageParam, search),
    initialPageParam: 0,
    getNextPageParam: (lastPage, pages) => {
      const loaded = pages.reduce((count, page) => count + page.items.length, 0);
      return loaded < lastPage.total ? loaded : undefined;
    },
    enabled: open && !disabled,
  });

  const userOptions = useMemo(() => {
    const users = new Map<string, UserOptionRecord>();
    for (const item of seedOptions) users.set(item.id, item);
    if (selectedUser) users.set(selectedUser.id, selectedUser);
    for (const page of usersQuery.data?.pages || []) {
      for (const item of page.items) users.set(item.id, item);
    }
    return [...users.values()].map((item) => ({
      label: `${item.display_name || item.username} (${item.username})`,
      value: item.id,
    }));
  }, [seedOptions, selectedUser, usersQuery.data]);

  function handlePopupScroll(event: UIEvent<HTMLDivElement>) {
    const target = event.currentTarget;
    if (
      target.scrollTop + target.clientHeight >= target.scrollHeight &&
      usersQuery.hasNextPage &&
      !usersQuery.isFetchingNextPage
    ) {
      void usersQuery.fetchNextPage();
    }
  }

  return (
    <Select<string>
      {...selectProps}
      value={value}
      disabled={disabled}
      showSearch
      filterOption={false}
      options={[...fixedOptions, ...userOptions]}
      loading={usersQuery.isLoading || usersQuery.isFetchingNextPage}
      onSearch={setSearchInput}
      onPopupScroll={handlePopupScroll}
      onOpenChange={(nextOpen) => {
        setOpen(nextOpen);
        onOpenChange?.(nextOpen);
      }}
      onChange={(nextValue, option) => {
        setSelectedUser(
          seedOptions
            .concat(...(usersQuery.data?.pages || []).map((page) => page.items))
            .find((item) => item.id === nextValue),
        );
        onChange?.(nextValue, option);
      }}
    />
  );
}
