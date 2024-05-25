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

export interface DeviceLineProperties {
    color: Color
    hidden: boolean
    name: string
}

export const tooltipPlugin = (allDevicesLineProperties: Map<string, DeviceLineProperties>) => {
    const tooltip = document.createElement('div')
    tooltip.className = 'u-plot-tooltip'
    tooltip.style.display = 'none'
    tooltip.style.position = 'absolute'
    tooltip.style.background = 'color-mix(in srgb, var(--surface-card) 94%, transparent)'
    tooltip.style.border = '2px solid var(--cc-bg-three)'
    tooltip.style.borderRadius = 'var(--border-radius)'
    tooltip.style.color = 'var(--text-color)'
    tooltip.style.padding = 'var(--inline-spacing)'
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
                    let lowerPercentLimit: number = 110
                    let upperPercentLimit: number = -1
                    const rpmScaleMax: undefined | number = u.scales[SCALE_KEY_RPM]?.max
                    let lowerRpmLimit: number = 10_000
                    let upperRpmLimit: number = -1
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
                            // Calculate Cursor values once for all series:
                            if (isPercentScale && upperPercentLimit == -1) {
                                const percentCursorValue = u.posToVal(c.top ?? 0, SCALE_KEY_PERCENT)
                                lowerPercentLimit = percentCursorValue - 2
                                upperPercentLimit = percentCursorValue + 2
                                if (lowerPercentLimit < 2) {
                                    // keeps upper and lower boundaries within the canvas area
                                    lowerPercentLimit = 0
                                    upperPercentLimit = 4
                                } else if (upperPercentLimit > 98) {
                                    lowerPercentLimit = 96
                                    upperPercentLimit = 120
                                }
                            } else if (
                                !isPercentScale &&
                                upperRpmLimit == -1 &&
                                rpmScaleMax != null
                            ) {
                                // Calculate RPM series' value range once for all series
                                const rpmCursorValue = u.posToVal(c.top ?? 0, SCALE_KEY_RPM)
                                const rpmSeriesValueRange = rpmScaleMax * 0.04
                                const rpmSeriesValueRangeSplit = rpmSeriesValueRange / 2
                                lowerRpmLimit = rpmCursorValue - rpmSeriesValueRangeSplit
                                upperRpmLimit = rpmCursorValue + rpmSeriesValueRangeSplit
                                if (lowerRpmLimit < rpmSeriesValueRangeSplit) {
                                    // keeps upper and lower boundaries within the canvas area
                                    lowerRpmLimit = 0
                                    upperRpmLimit = rpmSeriesValueRange
                                } else if (upperRpmLimit > rpmScaleMax - rpmSeriesValueRangeSplit) {
                                    lowerRpmLimit = rpmScaleMax - rpmSeriesValueRange
                                    upperRpmLimit = rpmScaleMax
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
                                !isPercentScale &&
                                (seriesValue < lowerRpmLimit || seriesValue > upperRpmLimit)
                            ) {
                                // out of range for the rpm scale
                                continue
                            }
                            const lineName = allDevicesLineProperties.get(series.label!)?.name
                            const lineValue: string =
                                series.label!.endsWith('fan') || series.label!.endsWith('temp')
                                    ? seriesValue.toFixed(1)
                                    : seriesValue.toString()
                            let suffix = ''
                            if (series.label!.endsWith('temp')) {
                                suffix = '°'
                            } else if (series.label!.endsWith('rpm')) {
                                suffix = 'rpm'
                            } else if (series.label!.endsWith('freq')) {
                                suffix = 'mhz'
                            } else {
                                suffix = '%'
                            }
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
                        backgroundColor:
                            'color-mix(in srgb, var(--primary-color) 30%, transparent)',
                    })
                    uPlot.assign(highlightEl2.style, {
                        pointerEvents: 'none',
                        display: 'none',
                        position: 'absolute',
                        left: 0,
                        top: 0,
                        height: '100%',
                        backgroundColor:
                            'color-mix(in srgb, var(--primary-color) 10%, transparent)',
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
                    const scale_key = hasPercentScale ? SCALE_KEY_PERCENT : SCALE_KEY_RPM
                    const scale_max = u.scales[scale_key].max!
                    const scale_2_percent = hasPercentScale ? 2 : u.scales[scale_key].max! * 0.02
                    const percentCursorValue = u.posToVal(u.cursor.top ?? 0, scale_key)
                    const topCursorValue = Math.min(
                        Math.max(percentCursorValue + scale_2_percent, scale_2_percent * 2),
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
                    const xMin = u.data[0].at(0)!
                    const xMax = u.data[0].at(-1)!
                    const xRange = xMax - xMin

                    const left: number = u.cursor.left!

                    const leftPct = left / rect.width
                    const xVal = u.posToVal(left, 'x')
                    const timeScale: uPlot.Scale = u.scales.x
                    const oxRange = timeScale.max! - timeScale.min!

                    const nxRange: number = e.deltaY < 0 ? oxRange * factor : oxRange / factor
                    let nxMin: number = xVal - leftPct * nxRange
                    let nxMax: number = nxMin + nxRange
                    ;[nxMin, nxMax] = clamp(nxRange, nxMin, nxMax, xRange, xMin, xMax, timeScale)

                    u.batch(() => {
                        u.setScale('x', {
                            min: nxMin,
                            max: nxMax,
                        })
                    })
                })
            },
        },
    }
}
