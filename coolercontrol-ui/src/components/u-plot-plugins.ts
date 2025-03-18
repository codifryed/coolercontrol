/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

import uPlot from 'uplot'
import type { Color } from '@/models/Device.ts'

export const SCALE_KEY_PERCENT: string = '%'
export const SCALE_KEY_RPM: string = 'rpm'
export const SCALE_KEY_WATTS: string = 'W'

export interface DeviceLineProperties {
    color: Color
    name: string
}

export const tooltipPlugin = (allDevicesLineProperties: Map<string, DeviceLineProperties>) => {
    const tooltip = document.createElement('div')
    tooltip.className = 'u-plot-tooltip'
    tooltip.style.display = 'none'
    tooltip.style.position = 'absolute'
    tooltip.style.background = 'rgba(var(--colors-bg-one) / 0.94)'
    tooltip.style.border = '1px solid rgb(var(--colors-border-one))'
    tooltip.style.borderRadius = '0.5rem'
    tooltip.style.color = 'rgb(var(--colors-text-color))'
    tooltip.style.padding = '0.5rem'
    let tooltipVisible: boolean = false

    const showTooltip = () => {
        if (!tooltipVisible) {
            tooltip.style.display = 'block'
            tooltipVisible = true
        }
    }

    const hideTooltip = () => {
        if (tooltipVisible) {
            tooltip.style.display = 'none'
            tooltipVisible = false
        }
    }

    const setTooltip = (u: uPlot, contentHTML: string) => {
        if (
            u.cursor.top == null ||
            u.cursor.top < 0 ||
            u.cursor.left == null ||
            u.cursor.left < 0
        ) {
            hideTooltip()
            return
        }
        tooltip.innerHTML = contentHTML
        showTooltip() // need to show the tooltip before getting the element's size
        // handle tooltip positioning:
        const leftOffset =
            u.cursor.left! < u.width * 0.9 - tooltip.offsetWidth ? 10 : -(tooltip.offsetWidth + 10)
        const topOffset =
            u.cursor.top! < u.height * 0.9 - tooltip.offsetHeight
                ? 10
                : -(tooltip.offsetHeight + 10)
        tooltip.style.top = topOffset + u.cursor.top + 'px'
        tooltip.style.left = leftOffset + u.cursor.left + 'px'
    }

    return {
        hooks: {
            init: [
                (u: uPlot, _: uPlot.Options, __: uPlot.AlignedData) => {
                    u.over.appendChild(tooltip)
                },
            ],
            setCursor: [
                (u: uPlot) => {
                    const seriesTexts: Array<string> = []
                    const c = u.cursor
                    const percentScaleMax: undefined | number = u.scales[SCALE_KEY_PERCENT]?.max
                    const percentScaleMin: undefined | number = u.scales[SCALE_KEY_PERCENT]?.min
                    let lowerPercentLimit: number = 210
                    let upperPercentLimit: number = -1
                    const rpmScaleMax: undefined | number = u.scales[SCALE_KEY_RPM]?.max
                    const rpmScaleMin: undefined | number = u.scales[SCALE_KEY_RPM]?.min
                    let lowerRpmLimit: number = 4_294_967_295 // Max u32 value from daemon
                    let upperRpmLimit: number = -1
                    const wattScaleMax: undefined | number = u.scales[SCALE_KEY_WATTS]?.max
                    const wattScaleMin: undefined | number = u.scales[SCALE_KEY_WATTS]?.min
                    let lowerWattLimit: number = 4_294_967_295 // Max u32 value from daemon
                    let upperWattLimit: number = -1
                    for (const [i, series] of u.series.entries()) {
                        if (i == 0) {
                            // time series
                            continue
                        }
                        if (series.show) {
                            const seriesValue: number = u.data[i][c.idx!]!
                            if (seriesValue == null) {
                                // when leaving the canvas area during an update,
                                // the value can be undefined
                                continue
                            }
                            const isPercentScale: boolean = series.scale == SCALE_KEY_PERCENT
                            const isRpmScale: boolean = series.scale == SCALE_KEY_RPM
                            const isWattScale: boolean = series.scale == SCALE_KEY_WATTS
                            // Calculate Cursor values once for all series:
                            if (
                                upperPercentLimit == -1 &&
                                isPercentScale &&
                                percentScaleMax != null &&
                                percentScaleMin != null
                            ) {
                                // Calculate Percent series' value range once for all series
                                const percentCursorValue = u.posToVal(c.top ?? 0, SCALE_KEY_PERCENT)
                                const percentScaleRange = percentScaleMax - percentScaleMin
                                const percentSeriesValueRange = percentScaleRange * 0.04
                                const percentSeriesValueRangeSplit = percentSeriesValueRange / 2
                                lowerPercentLimit =
                                    percentCursorValue - percentSeriesValueRangeSplit
                                upperPercentLimit =
                                    percentCursorValue + percentSeriesValueRangeSplit
                                // keeps upper and lower boundaries within the canvas area:
                                if (
                                    lowerPercentLimit <
                                    percentScaleMin + percentSeriesValueRangeSplit
                                ) {
                                    lowerPercentLimit = percentScaleMin
                                    upperPercentLimit = percentScaleMin + percentSeriesValueRange
                                } else if (
                                    upperPercentLimit >
                                    percentScaleMax - percentSeriesValueRangeSplit
                                ) {
                                    lowerPercentLimit = percentScaleMax - percentSeriesValueRange
                                    upperPercentLimit = percentScaleMax
                                }
                            } else if (
                                upperRpmLimit == -1 &&
                                isRpmScale &&
                                rpmScaleMax != null &&
                                rpmScaleMin != null
                            ) {
                                // Calculate RPM series' value range once for all series
                                const rpmCursorValue = u.posToVal(c.top ?? 0, SCALE_KEY_RPM)
                                const rpmScaleRange = rpmScaleMax - rpmScaleMin
                                const rpmSeriesValueRange = rpmScaleRange * 0.04
                                const rpmSeriesValueRangeSplit = rpmSeriesValueRange / 2
                                lowerRpmLimit = rpmCursorValue - rpmSeriesValueRangeSplit
                                upperRpmLimit = rpmCursorValue + rpmSeriesValueRangeSplit
                                // keeps upper and lower boundaries within the canvas area:
                                if (lowerRpmLimit < rpmScaleMin + rpmSeriesValueRangeSplit) {
                                    lowerRpmLimit = rpmScaleMin
                                    upperRpmLimit = rpmScaleMin + rpmSeriesValueRange
                                } else if (upperRpmLimit > rpmScaleMax - rpmSeriesValueRangeSplit) {
                                    lowerRpmLimit = rpmScaleMax - rpmSeriesValueRange
                                    upperRpmLimit = rpmScaleMax
                                }
                            } else if (
                                upperWattLimit == -1 &&
                                isWattScale &&
                                wattScaleMax != null &&
                                wattScaleMin != null
                            ) {
                                // Calculate Watt series' value range once for all series
                                const wattCursorValue = u.posToVal(c.top ?? 0, SCALE_KEY_WATTS)
                                const wattScaleRange = wattScaleMax - wattScaleMin
                                const wattSeriesValueRange = wattScaleRange * 0.04
                                const wattSeriesValueRangeSplit = wattSeriesValueRange / 2
                                lowerWattLimit = wattCursorValue - wattSeriesValueRangeSplit
                                upperWattLimit = wattCursorValue + wattSeriesValueRangeSplit
                                // keeps upper and lower boundaries within the canvas area:
                                if (lowerWattLimit < wattScaleMin + wattSeriesValueRangeSplit) {
                                    lowerWattLimit = wattScaleMin
                                    upperWattLimit = wattScaleMin + wattSeriesValueRange
                                } else if (
                                    upperWattLimit >
                                    wattScaleMax - wattSeriesValueRangeSplit
                                ) {
                                    lowerWattLimit = wattScaleMax - wattSeriesValueRange
                                    upperWattLimit = wattScaleMax
                                }
                            }
                            // Check if series is in range of the cursor
                            if (
                                isPercentScale &&
                                (seriesValue < lowerPercentLimit || seriesValue > upperPercentLimit)
                            ) {
                                // Out of range for the percent scale
                                continue
                            }
                            if (
                                isRpmScale &&
                                (seriesValue < lowerRpmLimit || seriesValue > upperRpmLimit)
                            ) {
                                // out of range for the rpm scale
                                continue
                            }
                            if (
                                isWattScale &&
                                (seriesValue < lowerWattLimit || seriesValue > upperWattLimit)
                            ) {
                                // out of range for the watt scale
                                continue
                            }
                            // @ts-ignore
                            const lineName = allDevicesLineProperties.get(series.label!)?.name
                            let lineValue: string = ''
                            let suffix: string = ''
                            // @ts-ignore
                            if (series.label!.endsWith('duty')) {
                                lineValue = seriesValue.toString()
                                suffix = '%'
                                // @ts-ignore
                            } else if (series.label!.endsWith('temp')) {
                                lineValue = seriesValue.toFixed(1)
                                suffix = '°'
                                // @ts-ignore
                            } else if (series.label!.endsWith('load')) {
                                lineValue = seriesValue.toString()
                                suffix = '%'
                                // @ts-ignore
                            } else if (series.label!.endsWith('freq')) {
                                const frequencyPrecision = seriesValue.toString().includes('.')
                                    ? 1000
                                    : 1
                                if (frequencyPrecision === 1) {
                                    lineValue = seriesValue.toString()
                                    suffix = 'Mhz'
                                } else {
                                    lineValue = seriesValue.toFixed(2)
                                    suffix = 'Ghz'
                                }
                                // @ts-ignore
                            } else if (series.label!.endsWith('rpm')) {
                                const frequencyPrecision = seriesValue.toString().includes('.')
                                    ? 1000
                                    : 1
                                suffix = 'rpm'
                                lineValue = (seriesValue * frequencyPrecision).toFixed(0)
                                // @ts-ignore
                            } else if (series.label!.endsWith('watts')) {
                                lineValue = seriesValue.toFixed(1)
                                suffix = 'W'
                            }
                            // @ts-ignore
                            const lineColor = allDevicesLineProperties.get(series.label!)?.color
                            seriesTexts.push(
                                `<tr><td><i class="pi pi-minus" style="color:${lineColor};"/></td><td>${lineName}&nbsp;</td><td>${lineValue} ${suffix}</td></tr>`,
                            )
                        }
                    }
                    if (seriesTexts.length > 0) {
                        seriesTexts.splice(0, 0, '<table style="white-space: nowrap;">')
                        seriesTexts.push('</table>')
                        setTooltip(u, seriesTexts.join(''))
                    } else {
                        hideTooltip()
                    }
                },
            ],
        },
    }
}

export const columnHighlightPlugin = () => {
    const highlightEl: HTMLElement = document.createElement('div')
    const highlightEl2: HTMLElement = document.createElement('div')
    return {
        hooks: {
            init: [
                (u: uPlot) => {
                    const underEl: HTMLElement = u.under
                    const overEl: HTMLElement = u.over
                    uPlot.assign(highlightEl.style, {
                        pointerEvents: 'none',
                        display: 'none',
                        position: 'absolute',
                        left: 0,
                        top: 0,
                        height: '4%',
                        backgroundColor: 'rgba(var(--colors-accent) / 0.3)',
                    })
                    uPlot.assign(highlightEl2.style, {
                        pointerEvents: 'none',
                        display: 'none',
                        position: 'absolute',
                        left: 0,
                        top: 0,
                        height: '100%',
                        backgroundColor: 'rgba(var(--colors-accent) / 0.1)',
                    })
                    underEl.appendChild(highlightEl)
                    underEl.appendChild(highlightEl2)
                    // show/hide highlight on enter/exit
                    overEl.addEventListener('mouseenter', () => {
                        highlightEl.style.display = 'block'
                        highlightEl2.style.display = 'block'
                    })
                    overEl.addEventListener('mouseleave', () => {
                        highlightEl.style.display = 'none'
                        highlightEl2.style.display = 'none'
                    })
                },
            ],
            setCursor: [
                (u: uPlot) => {
                    if (u.series.length < 2) {
                        // in case there are no series-types selected
                        return
                    }
                    const currIdx = u.cursor.idx ?? 0
                    const [iMin, iMax] = u.series[0].idxs!
                    const dx = iMax - iMin
                    const width = u.bbox.width / dx / devicePixelRatio
                    const xVal = u.scales.x.distr == 2 ? currIdx : u.data[0][currIdx]
                    const left = u.valToPos(xVal, 'x') - width / 2

                    highlightEl.style.transform = 'translateX(' + Math.round(left) + 'px)'
                    highlightEl2.style.transform = 'translateX(' + Math.round(left) + 'px)'
                    highlightEl.style.width = Math.round(Math.max(width, 5)) + 'px'
                    highlightEl2.style.width = Math.round(Math.max(width, 5)) + 'px'

                    const hasPercentScale: boolean = u.series[1].scale == SCALE_KEY_PERCENT
                    const hasRpmScale: boolean = u.series[1].scale == SCALE_KEY_RPM
                    const scale_key = hasPercentScale
                        ? SCALE_KEY_PERCENT
                        : hasRpmScale
                          ? SCALE_KEY_RPM
                          : SCALE_KEY_WATTS
                    const scale_max = u.scales[scale_key].max!
                    const scale_min = u.scales[scale_key].min!
                    const scale_range = scale_max - scale_min
                    const scale_2_percent = scale_range * 0.02
                    const percentCursorValue = u.posToVal(u.cursor.top ?? 0, scale_key)
                    const topCursorValue = Math.min(
                        Math.max(
                            percentCursorValue + scale_2_percent,
                            scale_min + scale_2_percent * 2,
                        ),
                        scale_max,
                    )
                    const topCursorPos = u.valToPos(topCursorValue, scale_key)
                    highlightEl.style.top = topCursorPos + 'px'
                },
            ],
        },
    }
}

export const mouseWheelZoomPlugin = () => {
    return {
        hooks: {
            ready: (u: uPlot) => {
                // This delay fixes a bug on initial startup where it appears that rendering hasn't
                // completely finished and scrolling has incorrect bounds.
                function sleep(ms: number) {
                    return new Promise((resolve) => setTimeout(resolve, ms))
                }
                sleep(200).then(() => {
                    const factor = 0.75
                    function clamp(
                        nRange: number,
                        nMin: number,
                        nMax: number,
                        xRange: number,
                        xMin: number,
                        xMax: number,
                        timeScale: uPlot.Scale,
                    ) {
                        if (nRange < 10) {
                            // 10 seconds
                            nMin = timeScale.min!
                            nMax = timeScale.max!
                        } else if (nRange > xRange) {
                            nMin = xMin
                            nMax = xMax
                        } else if (nMin < xMin) {
                            nMin = xMin
                            nMax = xMin + nRange
                        } else if (nMax > xMax) {
                            nMax = xMax
                            nMin = xMax - nRange
                        }
                        return [nMin, nMax]
                    }
                    const rect = u.over.getBoundingClientRect()
                    u.over.addEventListener('wheel', (e) => {
                        e.preventDefault()
                        const xMin = u.data[0][0]!
                        const xMax = u.data[0][u.data[0].length - 1]!
                        const xRange = xMax - xMin

                        const left: number = u.cursor.left!

                        const leftPct = left / rect.width
                        const xVal = u.posToVal(left, 'x')
                        const timeScale: uPlot.Scale = u.scales.x!
                        const oxRange = timeScale.max! - timeScale.min!

                        const nxRange: number = e.deltaY < 0 ? oxRange * factor : oxRange / factor
                        let nxMin: number = xVal - leftPct * nxRange
                        let nxMax: number = nxMin + nxRange
                        ;[nxMin, nxMax] = clamp(
                            nxRange,
                            nxMin,
                            nxMax,
                            xRange,
                            xMin,
                            xMax,
                            timeScale,
                        )

                        u.batch(() => {
                            u.setScale('x', {
                                min: nxMin,
                                max: nxMax,
                            })
                        })
                    })

                    // panning
                    u.over.addEventListener('contextmenu', (e) => {
                        // u.over.addEventListener('mousedown', (e) => {
                        //     if (e.button == 1) {
                        u.over.style.cursor = 'move'
                        e.preventDefault()

                        const xMin = u.data[0][0]!
                        const xMax = u.data[0][u.data[0].length - 1]!

                        const left0 = e.clientX
                        //	let top0 = e.clientY

                        const scXMin0 = u.scales.x.min!
                        const scXMax0 = u.scales.x.max!
                        const xRange = scXMax0 - scXMin0

                        const xUnitsPerPx = u.posToVal(1, 'x') - u.posToVal(0, 'x')

                        function onmove(e: any) {
                            e.preventDefault()

                            const left1 = e.clientX
                            //	let top1 = e.clientY

                            const dx = xUnitsPerPx * (left1 - left0)
                            let nMin = scXMin0 - dx
                            let nMax = scXMax0 - dx

                            // clamp:
                            if (nMin < xMin) {
                                nMin = xMin
                                // panning on edges, never allow range to be smaller
                                nMax = xMin + xRange
                            } else if (nMax > xMax) {
                                nMax = xMax
                                // panning on edges, never allow range to be smaller
                                nMin = xMax - xRange
                            }
                            u.batch(() => {
                                u.setScale('x', {
                                    min: nMin,
                                    max: nMax,
                                })
                            })
                        }

                        function onup(_: any) {
                            u.over.style.cursor = 'auto'
                            document.removeEventListener('mousemove', onmove)
                            document.removeEventListener('mouseup', onup)
                        }

                        document.addEventListener('mousemove', onmove)
                        document.addEventListener('mouseup', onup)
                        // }
                    })
                })
            },
        },
    }
}
