import type { ImgHTMLAttributes } from 'react';

export default function Image(props: ImgHTMLAttributes<HTMLImageElement> & { fill?: boolean; priority?: boolean }) {
  const { fill, priority: _priority, style, ...imageProps } = props;
  return <img {...imageProps} style={fill ? { ...style, position: 'absolute', inset: 0, width: '100%', height: '100%', objectFit: style?.objectFit ?? 'cover' } : style} />;
}
