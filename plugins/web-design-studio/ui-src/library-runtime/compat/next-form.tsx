import { forwardRef, type ComponentPropsWithoutRef } from 'react';

const NextForm = forwardRef<HTMLFormElement, ComponentPropsWithoutRef<'form'>>((props, ref) => <form ref={ref} {...props} />);
NextForm.displayName = 'NextForm';

export default NextForm;
