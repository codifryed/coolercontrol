export default {
    root: ({ props }) => ({
        class: [
            'inline-flex select-none align-bottom outline-transparent',
            'border border-border-one rounded-lg [&>button]:rounded-none',
            '[&>button:first-child]:border-r-none [&>button:first-child]:rounded-r-none [&>button:first-child]:rounded-tl-lg [&>button:first-child]:rounded-bl-lg',
            '[&>button:last-child]:border-l-none [&>button:first-child]:rounded-l-none [&>button:last-child]:rounded-tr-lg [&>button:last-child]:rounded-br-lg',

            // Invalid State
            {
                'border-red': props.invalid,
                'border-border-one': !props.invalid,
            },
        ],
    }),
}
