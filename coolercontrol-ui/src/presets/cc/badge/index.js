export default {
    root: ({ props }) => ({
        class: [
            // Font
            'font-bold',

            {
                'text-xs leading-[1.5rem]': props.size === null,
                'text-[0.625rem] leading-[1.25rem]': props.size === 'small',
                'text-lg leading-[2.25rem]': props.size === 'large',
                'text-2xl leading-[3rem]': props.size === 'xlarge',
            },

            // Alignment
            'text-center inline-block',

            // Size
            'p-0 px-1',
            {
                'w-2.5 h-2.5': props.value === null,
                'min-w-[1.5rem] h-[1.5rem]': props.value !== null && props.size === null,
                'min-w-[1.25rem] h-[1.25rem]': props.size === 'small',
                'min-w-[2.25rem] h-[2.25rem]': props.size === 'large',
                'min-w-[3rem] h-[3rem]': props.size === 'xlarge',
            },

            // Shape
            {
                'rounded-full': props.value?.length === 1,
                'rounded-[0.71rem]': props.value?.length !== 1,
            },

            // Color
            'text-success',
            // 'outline-none',
            {
                'bg-accent':
                    props.severity === null ||
                    props.severity === 'primary' ||
                    props.severity === 'secondary',
                'bg-success': props.severity === 'success',
                'bg-info': props.severity === 'info',
                'bg-warning': props.severity === 'warn',
                'bg-error': props.severity === 'error',
                'text-text-color bg-bg-one': props.severity === 'contrast',
            },
        ],
    }),
}
