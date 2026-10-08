/** Explicit cockpit layout and element styles. CSS is emitted only by StyleX. */
import * as stylex from "@stylexjs/stylex";
import { colors, layout, rounded, spacing, typography } from "./tokens.stylex";
export const cockpitStyles = stylex.create({
	element: {
		boxSizing: "border-box",
	},
	html: {
		margin: "0",
	},
	body: {
		margin: "0",
		background: colors.paper,
		color: colors.ink,
		fontFamily: typography.bodyFamily,
		fontSize: typography.bodySize,
		lineHeight: typography.bodyLineHeight,
		WebkitFontSmoothing: "antialiased",
		textRendering: "optimizeLegibility",
	},
	h1: {
		fontFamily: typography.headlineFamily,
		fontWeight: 600,
		color: colors.ink,
		lineHeight: "1.25",
	},
	h2: {
		fontFamily: typography.headlineFamily,
		fontWeight: 600,
		color: colors.ink,
		lineHeight: "1.25",
	},
	h3: {
		fontFamily: typography.headlineFamily,
		fontWeight: 600,
		color: colors.ink,
		lineHeight: "1.25",
	},
	h4: {
		fontFamily: typography.headlineFamily,
		fontWeight: 600,
		color: colors.ink,
		lineHeight: "1.25",
	},
	code: {
		fontFamily: typography.monoFamily,
		overflowWrap: "anywhere",
	},
	amount: {
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		whiteSpace: "nowrap",
	},
	num: {
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
	},
	tdNum: {
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
	},
	mValue: {
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
	},
	focusVisible: {
		outline: { default: null, ":focus-visible": `2px solid ${colors.accent}` },
		outlineOffset: { default: null, ":focus-visible": "1px" },
	},
	appShell: {
		maxWidth: layout.maxWidth,
		margin: "0 auto",
		padding: {
			default: "0 0 48px",
			"@media (max-width: 640px)": `0 ${spacing.md} ${spacing.xl}`,
		},
		paddingInline: { default: null, "@media (max-width: 639px)": "0" },
	},
	topbar: {
		display: { default: "flex", "@media print": "none" },
		alignItems: "center",
		gap: { default: "16px", "@media (max-width: 639px)": "8px" },
		padding: { default: "12px 24px", "@media (max-width: 1023px)": "8px 16px" },
		borderBottom: `1px solid ${colors.border}`,
		marginBottom: "0",
		flexWrap: { default: "wrap", "@media (max-width: 639px)": "wrap" },
		maxWidth: "1440px",
		marginInline: "auto",
	},
	topbarBuildVersion: {
		color: colors.inkMuted,
		fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
		fontSize: "0.72rem",
		fontWeight: "500",
		display: { default: null, "@media (max-width: 639px)": "none" },
	},
	topbarNav: {
		display: "flex",
		gap: spacing.md,
		marginLeft: { default: "auto", "@media (max-width: 640px)": "0" },
		fontSize: typography.sizeMd,
		width: { default: null, "@media (max-width: 640px)": "100%" },
	},
	accountMenu: {
		display: "flex",
		alignItems: "center",
		gap: spacing.xs,
		fontSize: typography.sizeSm,
		position: "relative",
		width: { default: null, "@media (max-width: 639px)": "100%" },
		flexWrap: { default: null, "@media (max-width: 639px)": "wrap" },
	},
	companySwitcher: {
		display: "flex",
		alignItems: "center",
		gap: spacing.xs,
		fontSize: typography.sizeSm,
	},
	accountIdentity: {
		display: "grid",
		lineHeight: "1.2",
	},
	accountEmail: {
		color: colors.inkMuted,
		maxWidth: { default: null, "@media (max-width: 639px)": "100%" },
		overflowWrap: { default: null, "@media (max-width: 639px)": "anywhere" },
	},
	accountRole: {
		color: colors.inkMuted,
		fontSize: typography.sizeXs,
	},
	accountError: {
		color: colors.danger,
		fontSize: typography.sizeXs,
		maxWidth: "220px",
	},
	accountPanelUl: {
		display: "grid",
		gap: spacing.sm,
		margin: `0 0 ${spacing.md}`,
		padding: "0",
		listStyle: "none",
	},
	accountPanelLi: {
		display: "flex",
		flexWrap: "wrap",
		alignItems: "center",
		justifyContent: "space-between",
		gap: spacing.sm,
	},
	accountPanelLiSpan: {
		display: "grid",
	},
	accountPanelSmall: {
		color: colors.inkMuted,
	},
	authPanel: {
		maxWidth: "460px",
		margin: "12vh auto 0",
		padding: spacing.lg,
	},
	authPanelH1: {
		marginTop: "0",
	},
	totpUri: {
		display: "block",
		overflowWrap: "anywhere",
		padding: spacing.sm,
		background: colors.paper,
		border: `1px solid ${colors.border}`,
		fontSize: typography.sizeXs,
	},
	recoveryCodes: {
		padding: spacing.sm,
		border: `1px solid ${colors.warning}`,
		background: colors.warningSoft,
	},
	recoveryCodesH3: {
		margin: "0",
	},
	recoveryCodesUl: {
		marginBottom: "0",
		fontFamily: typography.monoFamily,
	},
	pageHead: {
		display: "flex",
		alignItems: "center",
		justifyContent: "space-between",
		marginBottom: spacing.lg,
		gap: spacing.md,
		flexWrap: "wrap",
	},
	pageHeadH2: {
		margin: "0",
		fontSize: typography.sizeXl,
	},
	muted: {
		color: colors.inkMuted,
	},
	rowActions: {
		display: { default: "flex", "@media print": "none" },
		gap: spacing.xs,
		alignItems: "center",
		flexWrap: {
			default: null,
			"@media (max-width: 640px)": "wrap",
			"@media (max-width: 639px)": "wrap",
		},
	},
	groupOverview: {
		maxWidth: "920px",
	},
	groupAsOf: {
		display: "grid",
		gap: spacing.xxs,
		fontWeight: 500,
	},
	groupCard: {
		border: `1px solid ${colors.border}`,
		borderRadius: rounded.md,
		background: colors.paperRaised,
		padding: spacing.md,
		margin: `${spacing.md} 0`,
	},
	groupBlockers: {
		border: `1px solid ${colors.border}`,
		borderRadius: rounded.md,
		background: colors.paperRaised,
		padding: spacing.md,
		margin: `${spacing.md} 0`,
	},
	groupCardHeader: {
		display: "flex",
		justifyContent: "space-between",
		alignItems: "baseline",
		gap: spacing.sm,
	},
	groupCardH3: {
		marginTop: "0",
	},
	groupCardH4: {
		marginTop: "0",
		marginBottom: spacing.xs,
		fontSize: typography.sizeMd,
	},
	groupBlockersH3: {
		marginTop: "0",
	},
	groupCardUl: {
		marginTop: "0",
	},
	groupBlockersUl: {
		marginTop: "0",
	},
	groupReadiness: {
		fontSize: typography.sizeSm,
		fontWeight: 500,
	},
	groupReadinessBlocked: {
		color: colors.danger,
	},
	banner: {
		borderRadius: rounded.md,
		padding: `${spacing.sm} ${spacing.md}`,
		fontSize: typography.sizeMd,
		marginBottom: spacing.md,
		border: `1px solid ${colors.border}`,
	},
	bannerError: {
		background: colors.dangerSoft,
		borderColor: colors.danger,
		color: colors.ink,
	},
	bannerSuccess: {
		background: colors.successSoft,
		borderColor: colors.success,
		color: colors.ink,
	},
	bannerWarning: {
		background: colors.warningSoft,
		borderColor: colors.warning,
		color: colors.ink,
	},
	cvrFacts: {
		margin: `0 0 ${spacing.md}`,
	},
	cvrFactDt: {
		margin: "0",
	},
	cvrFactDd: {
		margin: "0",
		textAlign: "right",
	},
	formField: {
		display: "grid",
		gap: spacing.xxs,
		minWidth: "0",
	},
	formFieldLabel: {
		fontSize: typography.sizeSm,
		fontWeight: 600,
	},
	fieldHint: {
		margin: "0",
		fontSize: typography.sizeSm,
		color: colors.inkMuted,
	},
	fieldError: {
		margin: "0",
		fontSize: typography.sizeXs,
		color: colors.danger,
	},
	filterBarControls: {
		display: { default: "flex", "@media (max-width: 640px)": "grid" },
		flexWrap: "wrap",
		gap: spacing.sm,
		alignItems: "end",
		gridTemplateColumns: { default: null, "@media (max-width: 640px)": "1fr" },
	},
	filterBarAdvanced: {
		fontSize: typography.sizeSm,
	},
	filterBarAdvancedDiv: {
		display: { default: "flex", "@media (max-width: 640px)": "grid" },
		flexWrap: "wrap",
		gap: spacing.sm,
		marginTop: spacing.sm,
		gridTemplateColumns: { default: null, "@media (max-width: 640px)": "1fr" },
	},
	activeFilters: {
		display: "flex",
		flexWrap: "wrap",
		gap: spacing.sm,
		alignItems: "center",
		fontSize: typography.sizeSm,
	},
	pageState: {
		margin: `${spacing.md} 0`,
		padding: spacing.lg,
		border: `1px solid ${colors.border}`,
		background: colors.paperRaised,
	},
	pageStateH2: {
		margin: `0 0 ${spacing.sm}`,
	},
	pageStateP: {
		margin: `0 0 ${spacing.sm}`,
	},
	pageStateActions: {
		display: "flex",
		flexWrap: "wrap",
		gap: spacing.sm,
	},
	pageStateWarning: {
		borderLeft: `4px solid ${colors.warning}`,
	},
	pageStateBlocked: {
		borderLeft: `4px solid ${colors.warning}`,
	},
	pageStateError: {
		borderLeft: `4px solid ${colors.danger}`,
	},
	statusChip: {
		display: "inline-block",
		padding: `2px ${spacing.xs}`,
		border: `1px solid ${colors.border}`,
		fontSize: typography.sizeSm,
	},
	statusChipSuccess: {
		background: colors.successSoft,
		borderColor: colors.success,
	},
	statusChipWarning: {
		background: colors.warningSoft,
		borderColor: colors.warning,
	},
	statusChipDanger: {
		background: colors.dangerSoft,
		borderColor: colors.danger,
	},
	statusChipInfo: {
		background: colors.infoSoft,
	},
	pageHeaderActions: {
		display: "flex",
		flexWrap: "wrap",
		gap: spacing.xs,
		alignItems: "center",
	},
	metricCardH2: {
		margin: `0 0 ${spacing.xs}`,
		fontSize: typography.sizeMd,
	},
	metricCardValue: {
		fontFamily: typography.monoFamily,
		fontSize: typography.sizeXl,
		fontVariantNumeric: "tabular-nums",
	},
	metricCardP: {
		margin: `${spacing.xs} 0 0`,
	},
	responsiveTableShell: {
		padding: "0",
		overflow: "hidden",
	},
	responsiveTableScroll: {
		overflowX: { default: "auto", "@media (max-width: 520px)": "auto" },
		WebkitOverflowScrolling: {
			default: null,
			"@media (max-width: 520px)": "touch",
		},
		overflow: { default: null, "@media (max-width: 640px)": "visible" },
	},
	responsiveTable: {
		width: { default: "100%", "@media (max-width: 640px)": "100%" },
		minWidth: {
			default: null,
			"@media (max-width: 520px)": "0",
			"@media (max-width: 640px)": "0",
		},
		display: { default: null, "@media (max-width: 640px)": "block" },
	},
	responsiveTableTh: {
		textAlign: "left",
	},
	tableScroll: {
		overflowX: "auto",
		WebkitOverflowScrolling: {
			default: null,
			"@media (max-width: 520px)": "touch",
		},
	},
	responsiveTableThLastChild: {
		position: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "sticky" },
		},
		right: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "0" },
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	statementTableScrollTable: {
		minWidth: { default: null, "@media (max-width: 520px)": "640px" },
	},
	exceptionsViewTableTable: {
		minWidth: { default: null, "@media (max-width: 520px)": "640px" },
	},
	retentionViewTableTable: {
		minWidth: { default: null, "@media (max-width: 520px)": "640px" },
	},
	integrityViewTableTable: {
		minWidth: { default: null, "@media (max-width: 520px)": "640px" },
	},
	statementTableScrollTableThLastChild: {
		position: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "sticky" },
		},
		right: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "0" },
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	statementTableScrollTableTdLastChild: {
		position: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "sticky" },
		},
		right: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "0" },
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	exceptionsViewTableTableThLastChild: {
		position: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "sticky" },
		},
		right: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "0" },
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	exceptionsViewTableTableTdLastChild: {
		position: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "sticky" },
		},
		right: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "0" },
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	retentionViewTableTableThLastChild: {
		position: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "sticky" },
		},
		right: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "0" },
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	retentionViewTableTableTdLastChild: {
		position: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "sticky" },
		},
		right: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "0" },
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	integrityViewTableTableThLastChild: {
		position: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "sticky" },
		},
		right: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "0" },
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	integrityViewTableTableTdLastChild: {
		position: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "sticky" },
		},
		right: {
			default: null,
			"@media (max-width: 520px)": { default: null, ":last-child": "0" },
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	card: {
		background: colors.paperRaised,
		border: `1px solid ${colors.border}`,
		borderRadius: rounded.lg,
		padding: `${spacing.md} ${spacing.md}`,
	},
	companyGrid: {
		display: "grid",
		gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))",
		gap: spacing.md,
	},
	companyCard: {
		background: colors.paperRaised,
		border: `1px solid ${colors.border}`,
		borderLeft: `4px solid ${colors.borderStrong}`,
		borderRadius: rounded.lg,
		padding: spacing.md,
		display: "flex",
		flexDirection: "column",
		gap: spacing.sm,
	},
	companyCardLevelCritical: {
		borderLeftColor: colors.danger,
	},
	companyCardLevelWarning: {
		borderLeftColor: colors.warning,
	},
	companyCardLevelOk: {
		borderLeftColor: colors.success,
	},
	companyCardArchived: {
		opacity: "0.6",
	},
	companyCardCcHead: {
		display: "flex",
		justifyContent: "space-between",
		alignItems: "flex-start",
		gap: spacing.xs,
	},
	companyCardH3: {
		margin: "0",
		fontSize: typography.sizeLg,
	},
	companyCardCcCvr: {
		fontSize: typography.sizeSm,
		color: colors.inkMuted,
		fontFamily: typography.monoFamily,
	},
	rollupStrip: {
		display: "grid",
		gridTemplateColumns: {
			default: "repeat(4, 1fr)",
			"@media (max-width: 640px)": "1fr",
		},
		gap: { default: spacing.md, "@media (max-width: 640px)": spacing.sm },
		marginBottom: {
			default: spacing.xl,
			"@media (max-width: 640px)": spacing.lg,
		},
	},
	rollupCell: {
		background: colors.paperRaised,
		border: `1px solid ${colors.border}`,
		borderTop: `3px solid ${colors.borderStrong}`,
		borderRadius: rounded.lg,
		padding: `${spacing.md} ${spacing.lg}`,
	},
	rollupCellPos: {
		borderTopColor: colors.success,
	},
	rollupCellNeg: {
		borderTopColor: colors.danger,
	},
	rollupCellWarn: {
		borderTopColor: colors.warning,
	},
	rollupCellRollupLabel: {
		display: "block",
		fontSize: typography.sizeSm,
		textTransform: "uppercase",
		letterSpacing: "0.5px",
		color: colors.inkMuted,
	},
	rollupCellRollupValue: {
		display: "block",
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontSize: typography.sizeXl,
		fontWeight: 700,
		marginTop: spacing.xxs,
		lineHeight: "1.2",
	},
	rollupCellNegRollupValue: {
		color: colors.danger,
	},
	rollupCellWarnRollupValue: {
		color: colors.warning,
	},
	ccMetrics: {
		display: "grid",
		gridTemplateColumns: {
			default: "1fr 1fr",
			"@media (max-width: 640px)": "1fr 1fr",
		},
		gap: `${spacing.sm} ${spacing.md}`,
		paddingTop: spacing.xxs,
	},
	ccMetric: {
		display: "flex",
		flexDirection: "column",
		gap: "1px",
	},
	ccMetricMLabel: {
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
		color: colors.inkMuted,
	},
	ccMetricMValue: {
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontSize: typography.sizeLg,
		fontWeight: 600,
	},
	ccMetricPosMValue: {
		color: colors.success,
	},
	ccMetricNegMValue: {
		color: colors.danger,
	},
	ccMetricMSub: {
		fontSize: typography.sizeXs,
		color: colors.inkMuted,
	},
	ccTasks: {
		display: "flex",
		flexDirection: "column",
		gap: "2px",
		paddingTop: spacing.xxs,
		borderTop: `1px solid ${colors.border}`,
	},
	ccTaskNone: {
		fontSize: typography.sizeSm,
		color: colors.inkMuted,
	},
	ccTaskCount: {
		fontSize: typography.sizeSm,
		fontWeight: 600,
		color: colors.ink,
	},
	ccTaskLine: {
		fontSize: typography.sizeXs,
		color: colors.inkMuted,
	},
	ccTaskLineSevHigh: {
		color: colors.danger,
	},
	ccTaskLineSevMedium: {
		color: colors.warning,
	},
	flags: {
		display: "flex",
		flexWrap: "wrap",
		gap: spacing.xxs,
	},
	flag: {
		fontSize: typography.sizeSm,
		padding: `3px ${spacing.xs}`,
		borderRadius: rounded.sm,
		fontWeight: 500,
	},
	flagCritical: {
		background: colors.dangerSoft,
		color: colors.danger,
	},
	flagWarning: {
		background: colors.warningSoft,
		color: colors.warning,
	},
	flagOk: {
		background: colors.successSoft,
		color: colors.success,
	},
	flagNeutral: {
		background: colors.paper,
		color: colors.inkMuted,
		border: `1px solid ${colors.border}`,
	},
	badge: {
		fontSize: typography.sizeXs,
		padding: `2px ${spacing.xs}`,
		borderRadius: rounded.sm,
		background: colors.paper,
		border: `1px solid ${colors.border}`,
		color: colors.inkMuted,
	},
	form: {
		display: "flex",
		flexDirection: "column",
		gap: spacing.md,
		maxWidth: "460px",
	},
	formLabel: {
		display: "flex",
		flexDirection: "column",
		gap: spacing.xxs,
		fontSize: typography.sizeSm,
		color: colors.inkMuted,
	},
	modalHead: {
		display: "flex",
		alignItems: "center",
		justifyContent: "space-between",
		gap: spacing.sm,
	},
	modalBody: {
		fontSize: typography.sizeMd,
		color: colors.inkMuted,
	},
	modalBodyP: {
		margin: "0",
	},
	modalField: {
		display: "flex",
		flexDirection: "column",
		gap: spacing.xxs,
		fontSize: typography.sizeSm,
		color: colors.inkMuted,
	},
	modalFieldGrid: {
		display: "grid",
		gridTemplateColumns: "1fr 1fr",
		gap: spacing.sm,
	},
	modalFieldGridModalField: {
		minWidth: "0",
	},
	modalCheckbox: {
		flexDirection: "row",
		alignItems: "center",
		gap: spacing.xs,
	},
	invoiceLineRow: {
		display: "grid",
		gridTemplateColumns: "2fr 1fr 1fr",
		gap: spacing.sm,
		alignItems: "end",
		marginBottom: spacing.sm,
	},
	invoiceLineRowModalField: {
		minWidth: "0",
	},
	modalActions: {
		display: "flex",
		justifyContent: "flex-end",
		gap: spacing.sm,
		flexWrap: { default: null, "@media (max-width: 639px)": "wrap" },
	},
	section: {
		marginBottom: spacing.lg,
	},
	sectionH3: {
		fontSize: typography.sizeLg,
		margin: `0 0 ${spacing.sm}`,
	},
	tableData: {
		width: "100%",
		borderCollapse: "collapse",
		fontSize: typography.sizeSm,
	},
	tableDataTh: {
		textAlign: "left",
		padding: `${spacing.xs} ${spacing.sm}`,
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	tableDataTd: {
		textAlign: "left",
		padding: `${spacing.xs} ${spacing.sm}`,
		borderBottom: `1px solid ${colors.border}`,
	},
	tableDataTdNum: {
		textAlign: "right",
		fontVariantNumeric: "tabular-nums",
		fontFamily: typography.monoFamily,
	},
	emptyInline: {
		color: colors.inkMuted,
		fontSize: typography.sizeSm,
		padding: `${spacing.xs} 0`,
	},
	yearSelector: {
		display: "flex",
		alignItems: "center",
		gap: spacing.xs,
	},
	yearSelectorYsLabel: {
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
		color: colors.inkMuted,
	},
	chartCard: {
		padding: spacing.md,
	},
	statusGrid: {
		display: "grid",
		gridTemplateColumns: {
			default: "repeat(2, 1fr)",
			"@media (max-width: 640px)": "1fr",
		},
		gap: spacing.md,
	},
	statusCard: {
		display: "flex",
		flexDirection: "column",
		gap: spacing.xs,
	},
	statusCardH3: {
		margin: "0",
		fontSize: typography.sizeMd,
		textTransform: "uppercase",
		letterSpacing: "0.5px",
		color: colors.inkMuted,
		fontFamily: typography.bodyFamily,
		fontWeight: 600,
	},
	statusFigure: {
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontSize: typography.sizeXl,
		fontWeight: 700,
	},
	statusFigureStatusAlert: {
		color: colors.danger,
	},
	statusNote: {
		fontSize: typography.sizeSm,
		margin: "0",
	},
	entryDate: {
		fontFamily: typography.monoFamily,
		color: colors.inkMuted,
		whiteSpace: "nowrap",
	},
	entryText: {
		maxWidth: "0",
		overflow: "hidden",
		textOverflow: "ellipsis",
		whiteSpace: "nowrap",
	},
	archivedNotice: {
		textAlign: "center",
		padding: `${spacing["2xl"]} ${spacing.lg}`,
	},
	archivedNoticeH3: {
		margin: `0 0 ${spacing.xs}`,
		fontSize: typography.sizeLg,
	},
	trAccountRowHover: {
		background: { default: null, ":hover": colors.paper },
	},
	accountFilter: {
		display: "flex",
		alignItems: "center",
		justifyContent: "space-between",
		gap: spacing.md,
		flexWrap: "wrap",
		marginBottom: spacing.md,
		padding: `${spacing.sm} ${spacing.md}`,
		background: colors.paper,
		borderLeft: `3px solid ${colors.accent}`,
		borderRadius: rounded.md,
	},
	accountFilterP: {
		margin: "0",
		fontSize: typography.sizeSm,
	},
	docPosting: {
		display: "flex",
		flexDirection: "column",
		gap: spacing.xxs,
		alignItems: "flex-start",
	},
	docPostingText: {
		fontSize: typography.sizeSm,
	},
	multiYearCurrent: {
		marginLeft: spacing.xs,
		fontSize: typography.sizeSm,
	},
	companyTaskNavigation: {
		display: "flex",
		flexDirection: "column",
		gap: spacing.xs,
		borderBottom: `1px solid ${colors.border}`,
		marginBottom: spacing.lg,
	},
	companyAreas: {
		display: "flex",
		flexWrap: "wrap",
		gap: spacing.xxs,
		width: { default: "100%", "@media (max-width: 640px)": "100%" },
		paddingBottom: spacing.xs,
		borderBottom: `1px solid ${colors.border}`,
	},
	companyAreasAActive: {
		color: colors.ink,
		fontWeight: 600,
		borderColor: colors.accent,
		background: colors.paper,
	},
	companyYearControls: {
		display: "flex",
		justifyContent: {
			default: "flex-end",
			"@media (max-width: 640px)": "flex-start",
		},
		marginBottom: spacing.md,
	},
	statementAsof: {
		fontSize: typography.sizeSm,
		margin: `0 0 ${spacing.sm}`,
		fontFamily: typography.monoFamily,
	},
	statementCard: {
		padding: "0",
		overflow: "hidden",
	},
	tableStatementTable: {
		width: "100%",
	},
	statementSubtotalTd: {
		fontWeight: 600,
		borderTop: `1px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.borderStrong}`,
	},
	statementResultTd: {
		fontWeight: 700,
		fontSize: typography.sizeMd,
		borderTop: `2px solid ${colors.borderStrong}`,
		borderBottom: "none",
		paddingTop: spacing.sm,
		paddingBottom: spacing.sm,
	},
	statementResultPositiveTdNum: {
		color: colors.success,
	},
	statementResultNegativeTdNum: {
		color: colors.danger,
	},
	statementSubhead: {
		fontFamily: typography.headlineFamily,
		fontSize: typography.sizeMd,
		fontWeight: 600,
		margin: "0",
		padding: `${spacing.sm} ${spacing.md} 0`,
	},
	statementNote: {
		fontSize: typography.sizeSm,
		margin: `${spacing.xs} 0 ${spacing.xs}`,
		padding: `0 ${spacing.md}`,
	},
	statementCardHead: {
		display: "flex",
		alignItems: "center",
		justifyContent: "space-between",
		gap: spacing.sm,
		paddingRight: spacing.md,
	},
	statementCardHeadStatementSubhead: {
		paddingRight: "0",
	},
	rubrikNum: {
		display: "inline-block",
		marginRight: spacing.xs,
	},
	rubrikCopy: {
		display: "inline-flex",
		alignItems: "center",
		gap: spacing.xs,
	},
	rubrikCopyFeedback: {
		fontSize: typography.sizeXs,
		color: colors.success,
		fontWeight: 600,
	},
	statementCheck: {
		fontSize: typography.sizeSm,
		margin: `${spacing.sm} 0 0`,
		padding: `${spacing.xs} ${spacing.sm}`,
		borderRadius: rounded.md,
		border: `1px solid ${colors.border}`,
	},
	statementCheckOk: {
		background: colors.successSoft,
		borderColor: colors.success,
		color: colors.ink,
	},
	statementCheckAlert: {
		background: colors.dangerSoft,
		borderColor: colors.danger,
		color: colors.ink,
	},
	vatDeadline: {
		display: "flex",
		alignItems: "center",
		justifyContent: "space-between",
		gap: spacing.md,
		flexWrap: "wrap",
		marginTop: spacing.sm,
	},
	vatDeadlineLabel: {
		display: "block",
		fontSize: typography.sizeSm,
		color: colors.inkMuted,
	},
	vatDeadlineDate: {
		display: "block",
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontSize: typography.sizeLg,
		fontWeight: 700,
		color: colors.ink,
	},
	entryList: {
		listStyle: "none",
		margin: "0",
		padding: "0",
		display: "flex",
		flexDirection: "column",
		gap: spacing.xs,
	},
	entryItem: {
		background: colors.paperRaised,
		border: `1px solid ${colors.border}`,
		borderRadius: rounded.lg,
		overflow: "hidden",
	},
	entryItemOpen: {
		borderColor: colors.borderStrong,
	},
	entryCaret: {
		color: colors.inkMuted,
		fontSize: typography.sizeSm,
	},
	entryNo: {
		fontFamily: typography.monoFamily,
		fontSize: typography.sizeSm,
		color: colors.inkMuted,
		whiteSpace: "nowrap",
	},
	entrySummaryEntryTotal: {
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontWeight: 600,
		whiteSpace: "nowrap",
	},
	entryLines: {
		borderTop: `1px solid ${colors.border}`,
		background: colors.paper,
	},
	invoicesSummary: {
		marginBottom: spacing.lg,
	},
	statusFigureStatusIn: {
		color: colors.success,
	},
	statusFigureStatusOut: {
		color: colors.accent,
	},
	bankDiffBanner: {
		display: "flex",
		alignItems: "flex-start",
		gap: spacing.md,
		marginBottom: spacing.lg,
	},
	bankDiffBannerP: {
		margin: "0",
		fontSize: typography.sizeSm,
		color: colors.inkMuted,
	},
	bankDiffBannerFlag: {
		flexShrink: "0",
	},
	bankDiffBannerAlert: {
		background: colors.paper,
		borderLeft: `3px solid ${colors.warning}`,
	},
	bankDiffBannerOk: {
		borderLeft: `3px solid ${colors.success}`,
	},
	bankDiffBannerNeutral: {
		borderLeft: `3px solid ${colors.border}`,
	},
	bankDiffFigure: {
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontSize: typography.sizeXl,
		fontWeight: 700,
		color: colors.warning,
	},
	archiveBanner: {
		display: "flex",
		alignItems: "flex-start",
		gap: spacing.md,
		marginBottom: spacing.lg,
		background: colors.paper,
		borderLeft: `3px solid ${colors.warning}`,
	},
	archiveBannerP: {
		margin: "0",
		fontSize: typography.sizeSm,
		color: colors.inkMuted,
	},
	archiveBannerFlag: {
		flexShrink: "0",
	},
	archiveTag: {
		marginLeft: spacing.xs,
		fontSize: typography.sizeXs,
	},
	amountPositive: {
		color: colors.success,
	},
	amountNegative: {
		color: colors.accent,
	},
	entrySummaryEntryDate: {
		display: { default: null, "@media (max-width: 640px)": "none" },
	},
	responsiveTableThead: {
		position: { default: null, "@media (max-width: 640px)": "absolute" },
		width: { default: null, "@media (max-width: 640px)": "1px" },
		height: { default: null, "@media (max-width: 640px)": "1px" },
		overflow: { default: null, "@media (max-width: 640px)": "hidden" },
		clip: { default: null, "@media (max-width: 640px)": "rect(0 0 0 0)" },
	},
	responsiveTableTbody: {
		display: { default: null, "@media (max-width: 640px)": "block" },
		width: { default: null, "@media (max-width: 640px)": "100%" },
		minWidth: { default: null, "@media (max-width: 640px)": "0" },
	},
	responsiveTableTr: {
		display: { default: null, "@media (max-width: 640px)": "block" },
		width: { default: null, "@media (max-width: 640px)": "100%" },
		minWidth: { default: null, "@media (max-width: 640px)": "0" },
		padding: { default: null, "@media (max-width: 640px)": spacing.sm },
		borderBottom: {
			default: null,
			"@media (max-width: 640px)": `1px solid ${colors.border}`,
		},
	},
	helpViewHelpGrid: {
		display: "grid",
		gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
		gap: spacing.md,
		marginTop: spacing.md,
	},
	helpViewHelpGridCard: {
		padding: spacing.md,
	},
	helpViewHelpGridCardH3: {
		marginTop: "0",
		fontSize: typography.sizeLg,
	},
	helpViewHelpGridUl: {
		margin: "0",
		paddingLeft: spacing.md,
	},
	helpViewHelpGridOl: {
		margin: "0",
		paddingLeft: spacing.md,
	},
	helpViewHelpGridLiNotFirstChild: {
		marginTop: { default: null, ":not(:first-child)": spacing.xs },
	},
	workspaceLayout: {
		gridTemplateColumns: "minmax(0, 1fr)",
	},
	globalNavigationSummary: {
		cursor: "pointer",
		padding: "8px 12px",
		border: `1px solid ${colors.borderStrong}`,
		borderRadius: "4px",
		listStyle: "none",
	},
	globalLinks: {
		display: "grid",
		minWidth: "220px",
		gap: "8px",
		padding: "16px",
		background: colors.paperRaised,
		border: `1px solid ${colors.border}`,
		position: "absolute",
		zIndex: "30",
		right: "16px",
	},
	statement: {
		minWidth: "0",
	},
	tableWrap: {
		maxWidth: "100%",
		overflowX: "auto",
		overflow: { default: null, "@media print": "visible" },
	},
	filterBar: {
		display: { default: "flex", "@media (max-width: 639px)": "grid" },
		flexWrap: "wrap",
		alignItems: { default: "flex-end", "@media (max-width: 639px)": "start" },
		gap: "12px",
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "repeat(2, minmax(0, 1fr))",
		},
	},
	uiFilterFields: {
		display: { default: "flex", "@media (max-width: 639px)": "grid" },
		flexWrap: "wrap",
		alignItems: "flex-end",
		gap: "12px",
		marginTop: { default: null, "@media (max-width: 639px)": "12px" },
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(0, 1fr)",
		},
	},
	filterBarLabel: {
		display: "grid",
		gap: "4px",
		minWidth: { default: "140px", "@media (max-width: 639px)": "0" },
		flex: "1 1 140px",
	},
	workflowWithDocument: {
		display: "grid",
		gridTemplateColumns: {
			default: "minmax(0, 1fr) minmax(0, 1.5fr)",
			"@media (max-width: 1023px)": "minmax(0, 1fr)",
		},
		gap: "24px",
		alignItems: "start",
	},
	workflowForm: {
		display: "grid",
		gridTemplateColumns: "minmax(0, 1fr)",
		gap: spacing.md,
		minWidth: "0",
	},
	workflowForm2: {
		minWidth: "0",
		maxWidth: "100%",
	},
	formGrid: {
		display: "grid",
		gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 260px), 1fr))",
		gap: spacing.md,
	},
	formGridLabel: {
		display: "grid",
		gap: spacing.xxs,
		minWidth: "0",
	},
	fullWidth: {
		gridColumn: "1 / -1",
	},
	workflowProgress: {
		display: "flex",
		flexWrap: "wrap",
		gap: spacing.sm,
		listStyle: "none",
		padding: "0",
		marginBlock: spacing.md,
	},
	workflowDocument: {
		padding: "16px",
		background: colors.paperRaised,
		border: `1px solid ${colors.border}`,
		minWidth: "0",
	},
	documentPreview: {
		width: "100%",
		minHeight: "450px",
		border: `1px solid ${colors.border}`,
	},
	invoiceLines: {
		borderTop: `1px solid ${colors.border}`,
		paddingTop: "16px",
	},
	companyContext: {
		alignItems: { default: null, "@media (max-width: 639px)": "flex-start" },
		display: { default: null, "@media print": "none" },
	},
	companyContextLabel: {
		display: { default: null, "@media (max-width: 639px)": "grid" },
		gap: { default: null, "@media (max-width: 639px)": "4px" },
	},
	uiFilterFieldsExpanded: {
		display: { default: null, "@media (max-width: 639px)": "grid" },
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(0, 1fr)",
		},
	},
	dailyTableTbody: {
		display: { default: null, "@media (max-width: 639px)": "block" },
		width: { default: null, "@media (max-width: 639px)": "100%" },
	},
	dailyTableThead: {
		position: { default: null, "@media (max-width: 639px)": "absolute" },
		width: { default: null, "@media (max-width: 639px)": "1px" },
		height: { default: null, "@media (max-width: 639px)": "1px" },
		padding: { default: null, "@media (max-width: 639px)": "0" },
		margin: { default: null, "@media (max-width: 639px)": "-1px" },
		overflow: { default: null, "@media (max-width: 639px)": "hidden" },
		clipPath: { default: null, "@media (max-width: 639px)": "inset(50%)" },
		whiteSpace: { default: null, "@media (max-width: 639px)": "nowrap" },
	},
	dailyTableTr: {
		display: { default: null, "@media (max-width: 639px)": "grid" },
		border: {
			default: null,
			"@media (max-width: 639px)": `1px solid ${colors.border}`,
		},
		background: {
			default: null,
			"@media (max-width: 639px)": colors.paperRaised,
		},
		marginBlock: { default: null, "@media (max-width: 639px)": "12px" },
		padding: { default: null, "@media (max-width: 639px)": "8px" },
	},
	aside: {
		display: { default: null, "@media print": "none" },
	},
	cockpitLayout: {
		display: { default: null, "@media print": "block" },
	},
	lockMessage: {
		margin: "4px 0 0",
	},
	accountantExport: {
		marginTop: "24px",
		maxWidth: "460px",
	},
	sectionTitle: {
		marginTop: "0",
	},
	gallery: {
		maxWidth: "1100px",
		marginInline: "auto",
		padding: "24px",
	},
	gallerySection: {
		marginBlock: "32px",
	},
	galleryFields: {
		display: "grid",
		gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 260px), 1fr))",
		gap: "24px",
	},
	brandBuildVersion: {
		color: colors.inkMuted,
	},
	dailyListTbody: {
		display: { default: null, "@media (max-width: 639px)": "block" },
		width: { default: null, "@media (max-width: 639px)": "100%" },
	},
	dailyListThead: {
		position: { default: null, "@media (max-width: 639px)": "absolute" },
		width: { default: null, "@media (max-width: 639px)": "1px" },
		height: { default: null, "@media (max-width: 639px)": "1px" },
		overflow: { default: null, "@media (max-width: 639px)": "hidden" },
		clipPath: { default: null, "@media (max-width: 639px)": "inset(50%)" },
	},
	dailyListTr: {
		display: { default: null, "@media (max-width: 639px)": "grid" },
		padding: { default: null, "@media (max-width: 639px)": "8px" },
		marginBlock: { default: null, "@media (max-width: 639px)": "12px" },
		border: {
			default: null,
			"@media (max-width: 639px)": `1px solid ${colors.border}`,
		},
		background: {
			default: null,
			"@media (max-width: 639px)": colors.paperRaised,
		},
	},
	navigationDialogActions: {
		position: "sticky",
		top: "0",
		zIndex: "1",
		display: "flex",
		justifyContent: "flex-end",
		paddingBlock: spacing.xs,
		marginBottom: spacing.xs,
		background: colors.paperRaised,
		borderBottom: `1px solid ${colors.border}`,
	},
	buttonComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
	},
	selectComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
	},
	textareaComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
	},
	topbarBrandComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.ink,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		fontFamily: typography.headlineFamily,
		fontSize: {
			default: "22px",
			"@media (max-width: 639px)": "20px",
		},
		whiteSpace: "nowrap",
	},
	topbarNavAComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.inkMuted,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
	},
	aComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
	},
	modalFieldInputFocusComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
			":focus": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
			":focus": "-1px",
		},
		background: colors.paperRaised,
		border: `1px solid ${colors.borderStrong}`,
		borderRadius: rounded.md,
		color: colors.ink,
		padding: `${spacing.xs} ${spacing.sm}`,
		fontSize: typography.sizeMd,
		fontFamily: typography.bodyFamily,
	},
	modalFieldTextareaFocusComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
			":focus": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
			":focus": "-1px",
		},
		background: colors.paperRaised,
		border: `1px solid ${colors.borderStrong}`,
		borderRadius: rounded.md,
		color: colors.ink,
		padding: `${spacing.xs} ${spacing.sm}`,
		fontSize: typography.sizeMd,
		fontFamily: typography.bodyFamily,
		resize: "vertical",
	},
	flagComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		fontSize: typography.sizeSm,
		padding: `3px ${spacing.xs}`,
		borderRadius: rounded.sm,
		fontWeight: 500,
	},
	inputComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
	},
	companyAreasAComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.inkMuted,
		textDecoration: {
			default: "none",
			":hover": "none",
		},
		borderColor: {
			default: null,
			":hover": colors.borderStrong,
		},
		fontSize: typography.sizeMd,
		fontWeight: 500,
		padding: `${spacing.xs} ${spacing.sm}`,
		border: "1px solid transparent",
		borderRadius: rounded.sm,
		overflowWrap: "anywhere",
		minWidth: "0",
		background: "transparent",
		fontFamily: "inherit",
		textAlign: "left",
		transition: "color 0.12s ease",
		maxWidth: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
	},
	yearSelectorSelectFocusComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
			":focus": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
			":focus": "-1px",
		},
		background: colors.paperRaised,
		border: `1px solid ${colors.borderStrong}`,
		borderRadius: rounded.md,
		color: colors.ink,
		padding: `${spacing.xs} ${spacing.sm}`,
		fontSize: typography.sizeMd,
		fontFamily: typography.bodyFamily,
	},
	modalFieldSelectFocusComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
			":focus": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
			":focus": "-1px",
		},
		background: colors.paperRaised,
		border: `1px solid ${colors.borderStrong}`,
		borderRadius: rounded.md,
		color: colors.ink,
		padding: `${spacing.xs} ${spacing.sm}`,
		fontSize: typography.sizeMd,
		fontFamily: typography.bodyFamily,
	},
	invoiceLineRowButtonComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		gridColumn: "1 / -1",
		justifySelf: "start",
	},
	filterBarComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		display: {
			default: "flex",
			"@media (max-width: 639px)": "grid",
		},
		gap: "12px",
		flexWrap: "wrap",
		alignItems: {
			default: "flex-end",
			"@media (max-width: 639px)": "start",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "repeat(2, minmax(0, 1fr))",
		},
	},
	filterBarAdvancedSummaryComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		cursor: "pointer",
		minHeight: "44px",
		display: "inline-flex",
		alignItems: "center",
	},
	responsiveTableComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		width: {
			default: "100%",
			"@media (max-width: 640px)": "100%",
		},
		borderCollapse: "collapse",
		fontSize: typography.sizeSm,
		minWidth: {
			default: null,
			"@media (max-width: 520px)": "0",
			"@media (max-width: 640px)": "0",
		},
		display: {
			default: null,
			"@media (max-width: 640px)": "block",
		},
	},
	summaryComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
	},
	companyContextSelectComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		maxWidth: {
			default: null,
			"@media (max-width: 639px)": "100%",
		},
	},
	mobileMenuTriggerComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		display: {
			default: "none",
			"@media (max-width: 1023px)": "inline-flex",
		},
	},
	skipLinkFocusComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		position: "absolute",
		left: "16px",
		top: {
			default: "-100px",
			":focus": "8px",
		},
		background: colors.paperRaised,
		padding: "12px",
		zIndex: "100",
	},
	companySwitcherSelectComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		maxWidth: "190px",
	},
	accountRevokeComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		border: "0",
		background: "transparent",
		color: colors.inkMuted,
		fontSize: typography.sizeXs,
		textDecoration: "underline",
	},
	formInputFocusComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
			":focus": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
			":focus": "-1px",
		},
		background: colors.paperRaised,
		border: `1px solid ${colors.borderStrong}`,
		borderRadius: rounded.md,
		color: colors.ink,
		padding: `${spacing.xs} ${spacing.sm}`,
		fontSize: typography.sizeMd,
		fontFamily: typography.bodyFamily,
	},
	formSelectFocusComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
			":focus": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
			":focus": "-1px",
		},
		background: colors.paperRaised,
		border: `1px solid ${colors.borderStrong}`,
		borderRadius: rounded.md,
		color: colors.ink,
		padding: `${spacing.xs} ${spacing.sm}`,
		fontSize: typography.sizeMd,
		fontFamily: typography.bodyFamily,
	},
	modalCheckboxInputComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
			":focus": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
			":focus": "-1px",
		},
		background: colors.paperRaised,
		border: `1px solid ${colors.borderStrong}`,
		borderRadius: rounded.md,
		color: colors.ink,
		padding: "0",
		fontSize: typography.sizeMd,
		fontFamily: typography.bodyFamily,
		width: "auto",
	},
	statementBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	statementBtnComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	statementInputComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	statementSelectComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	dailyListTableComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		width: {
			default: "100%",
			"@media (max-width: 639px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 520px)": "0",
			"@media (max-width: 640px)": "0",
		},
		display: {
			default: null,
			"@media (max-width: 640px)": "block",
			"@media (max-width: 639px)": "block",
		},
		borderCollapse: "collapse",
		fontSize: typography.sizeSm,
	},
	dailyListTheadComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 640px)": "absolute",
			"@media (max-width: 639px)": "absolute",
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "1px",
			"@media (max-width: 639px)": "1px",
		},
		height: {
			default: null,
			"@media (max-width: 640px)": "1px",
			"@media (max-width: 639px)": "1px",
		},
		overflow: {
			default: null,
			"@media (max-width: 640px)": "hidden",
			"@media (max-width: 639px)": "hidden",
		},
		clip: {
			default: null,
			"@media (max-width: 640px)": "rect(0 0 0 0)",
		},
		clipPath: {
			default: null,
			"@media (max-width: 639px)": "inset(50%)",
		},
	},
	dailyListTrComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		display: {
			default: null,
			"@media (max-width: 640px)": "block",
			"@media (max-width: 639px)": "grid",
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		padding: {
			default: null,
			"@media (max-width: 640px)": spacing.sm,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: {
			default: null,
			"@media (max-width: 640px)": `1px solid ${colors.border}`,
		},
		marginBlock: {
			default: null,
			"@media (max-width: 639px)": "12px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": `1px solid ${colors.border}`,
		},
		background: {
			default: null,
			"@media (max-width: 639px)": colors.paperRaised,
		},
	},
	tableStatementTableThComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	tableStatementTableThComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "left",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	dailyListTbodyComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		display: {
			default: null,
			"@media (max-width: 640px)": "block",
			"@media (max-width: 639px)": "block",
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
			"@media (max-width: 639px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
	},
	dailyListTdComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		color: {
			default: colors.inkMuted,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: typography.sizeSm,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
			"@media (max-width: 639px)": "grid",
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
			"@media (max-width: 639px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
			"@media (max-width: 639px)": "anywhere",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyListTdComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
			"@media (max-width: 639px)": "grid",
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
			"@media (max-width: 639px)": "8px",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
			"@media (max-width: 639px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
			"@media (max-width: 639px)": "anywhere",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyListTdComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
			"@media (max-width: 639px)": "grid",
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
			"@media (max-width: 639px)": "8px",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
			"@media (max-width: 639px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
			"@media (max-width: 639px)": "anywhere",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: colors.inkMuted,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
		fontFamily: typography.monoFamily,
		whiteSpace: {
			default: "nowrap",
			"@media (max-width: 639px)": "normal",
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyListTdComposition4: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
			"@media (max-width: 639px)": "grid",
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
			"@media (max-width: 639px)": "8px",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
			"@media (max-width: 639px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
			"@media (max-width: 639px)": "anywhere",
		},
		textAlign: {
			default: "right",
			"@media (max-width: 639px)": "left",
		},
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyListTdComposition5: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: "flex",
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
			"@media print": "none",
			"@media (max-width: 639px)": "grid",
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: spacing.xs,
			"@media (max-width: 639px)": "8px",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
			"@media (max-width: 639px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
			"@media (max-width: 639px)": "anywhere",
		},
		textAlign: {
			default: "right",
			"@media (max-width: 639px)": "left",
		},
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
		alignItems: "center",
		flexWrap: {
			default: null,
			"@media (max-width: 640px)": "wrap",
			"@media (max-width: 639px)": "wrap",
		},
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	entrySummaryComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "-2px",
		},
		font: "inherit",
		cursor: "pointer",
		background: "none",
		display: "grid",
		gridTemplateColumns: {
			default: "auto auto auto 1fr auto",
			"@media (max-width: 640px)": "auto auto 1fr auto",
		},
		alignItems: "center",
		gap: spacing.sm,
		width: "100%",
		padding: `${spacing.sm} ${spacing.md}`,
		border: "none",
		textAlign: "left",
		fontFamily: typography.bodyFamily,
		fontSize: typography.sizeMd,
		color: colors.ink,
	},
	entryLinesTableStatementTableThComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		padding: `${spacing.xs} ${spacing.md}`,
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	entryLinesTableStatementTableThComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "left",
		padding: `${spacing.xs} ${spacing.md}`,
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	entryLinesTableStatementTableTdComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		padding: `${spacing.xs} ${spacing.md}`,
		borderBottom: `1px solid ${colors.border}`,
		fontFamily: typography.monoFamily,
		color: colors.inkMuted,
		whiteSpace: "nowrap",
	},
	entryLinesTableStatementTableTdComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		padding: `${spacing.xs} ${spacing.md}`,
		borderBottom: `1px solid ${colors.border}`,
	},
	entryLinesTableStatementTableTdComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "right",
		padding: `${spacing.xs} ${spacing.md}`,
		borderBottom: `1px solid ${colors.border}`,
	},
	rowActionsInputComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	statementTableScrollTableThComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	statementTableScrollTableTdComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	retentionViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	statusGridComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		marginBottom: spacing.lg,
		gridTemplateColumns: {
			default: "repeat(2, 1fr)",
			"@media (max-width: 880px)": "repeat(2, 1fr)",
			"@media (max-width: 640px)": "1fr",
		},
		display: "grid",
		gap: spacing.md,
	},
	dailyListTableComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		width: {
			default: "100%",
			"@media (max-width: 639px)": "100%",
		},
		borderCollapse: "collapse",
		fontSize: typography.sizeSm,
		minWidth: {
			default: null,
			"@media (max-width: 520px)": "640px",
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "block",
		},
	},
	statementButtonComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	statementTableScrollTableThComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	dailyListTdComposition6: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		color: {
			default: colors.inkMuted,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: typography.sizeSm,
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyListTdComposition7: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		color: {
			default: colors.inkMuted,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		whiteSpace: {
			default: "nowrap",
			"@media (max-width: 639px)": "normal",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyListTdComposition8: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
		color: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
	},
	dailyListTdComposition9: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: {
			default: "right",
			"@media (max-width: 639px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
		color: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
	},
	journalFilterFieldInputComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	journalFilterFieldSelectComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	dailyListTableComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		width: {
			default: "100%",
			"@media (max-width: 639px)": "100%",
		},
		borderCollapse: "collapse",
		fontSize: typography.sizeSm,
		display: {
			default: null,
			"@media (max-width: 639px)": "block",
		},
	},
	tableStatementTableThComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	tableStatementTableThComposition4: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	dailyListTdComposition10: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		color: {
			default: colors.inkMuted,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: typography.sizeSm,
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		borderBottom: `1px solid ${colors.border}`,
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyListTdComposition11: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		color: {
			default: colors.inkMuted,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		whiteSpace: {
			default: "nowrap",
			"@media (max-width: 639px)": "normal",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: `1px solid ${colors.border}`,
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyListTdComposition12: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: `1px solid ${colors.border}`,
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
		color: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
	},
	dailyListTdComposition13: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: {
			default: "right",
			"@media (max-width: 639px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: `1px solid ${colors.border}`,
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
		color: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
	},
	journalFilterFieldInputComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	journalFilterFieldSelectComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	rowActionsSelectComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	modalFieldInputFocusComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
			":focus": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
			":focus": "-1px",
		},
		minHeight: "44px",
		background: colors.paperRaised,
		border: `1px solid ${colors.borderStrong}`,
		borderRadius: rounded.md,
		color: colors.ink,
		padding: `${spacing.xs} ${spacing.sm}`,
		fontSize: typography.sizeMd,
		fontFamily: typography.bodyFamily,
	},
	rowActionsInputComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	dailyTableComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		width: {
			default: "100%",
			"@media (max-width: 639px)": "100%",
		},
		borderCollapse: "collapse",
		fontSize: typography.sizeSm,
		minWidth: {
			default: null,
			"@media (max-width: 520px)": "640px",
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "block",
		},
	},
	dailyTableTdComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		color: {
			default: colors.inkMuted,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: typography.sizeSm,
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		minWidth: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyTableTdComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: `1px solid ${colors.border}`,
		fontFamily: typography.monoFamily,
		color: {
			default: colors.inkMuted,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		whiteSpace: {
			default: "nowrap",
			"@media (max-width: 639px)": "normal",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		minWidth: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyTableTdComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		minWidth: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
		color: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
	},
	dailyTableTdComposition4: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		color: {
			default: colors.inkMuted,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		whiteSpace: {
			default: "nowrap",
			"@media (max-width: 639px)": "normal",
		},
		textAlign: {
			default: "left",
			"@media (max-width: 639px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		minWidth: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
	},
	dailyTableTdComposition5: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: {
			default: "right",
			"@media (max-width: 639px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 639px)": "8px",
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 639px)": "grid",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 639px)": "minmax(90px, 0.4fr) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 639px)": "8px",
		},
		border: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		minWidth: {
			default: null,
			"@media (max-width: 639px)": "0",
		},
		whiteSpace: {
			default: null,
			"@media (max-width: 639px)": "normal",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 639px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		font: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": `500 14px ${typography.bodyFamily}`,
			},
		},
		color: {
			default: null,
			"@media (max-width: 639px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
	},
	gdprViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	filterBarInputComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
		width: "100%",
	},
	gdprViewBtnComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	statementTableScrollTableTdComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		color: colors.inkMuted,
		fontSize: typography.sizeSm,
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		textAlign: "left",
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	tableStatementTableTdAccountNoComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		fontFamily: typography.monoFamily,
		color: colors.inkMuted,
		whiteSpace: "nowrap",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	accountLinkComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
			":focus-visible": "underline",
		},
	},
	tableDataTdNumComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "right",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	statementTableScrollTableTdComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontWeight: 600,
		borderTop: `1px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.border}`,
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	tableDataTdNumComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontWeight: 600,
		borderTop: `1px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.border}`,
		textAlign: "right",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	modalCheckboxInputComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		width: "auto",
		padding: "0",
	},
	pageButtonComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	pageInputComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	tableStatementTableTdComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		color: colors.inkMuted,
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	tableStatementTableTdAccountNoComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		fontFamily: typography.monoFamily,
		color: colors.inkMuted,
		whiteSpace: "nowrap",
	},
	tableStatementTableTdComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	tableDataTdNumComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "right",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	tableStatementTableTdNumMutedComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		color: colors.inkMuted,
		textAlign: "right",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	tableStatementTableTdComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontWeight: 700,
		fontSize: typography.sizeMd,
		borderTop: `2px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.border}`,
		paddingTop: spacing.sm,
		paddingBottom: spacing.sm,
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
	},
	tableDataTdNumComposition4: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontWeight: 700,
		fontSize: typography.sizeMd,
		borderTop: `2px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.border}`,
		paddingTop: spacing.sm,
		paddingBottom: spacing.sm,
		textAlign: "right",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
	},
	tableStatementTableTdComposition4: {
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
	},
	tdNumComposition: {
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
	},
	filterBarSelectComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		width: "100%",
	},
	filterBarInputComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		width: "100%",
	},
	rowActionsSelectComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	statementTableScrollTableComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		width: "100%",
		minWidth: {
			default: null,
			"@media (max-width: 520px)": "640px",
			"@media (max-width: 640px)": "0",
		},
		display: {
			default: null,
			"@media (max-width: 640px)": "block",
		},
		borderCollapse: "collapse",
		fontSize: typography.sizeSm,
	},
	statementTableScrollTableThComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		padding: `${spacing.xs} ${spacing.sm}`,
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	statementTableScrollTableTdComposition4: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: `${spacing.xs} ${spacing.sm}`,
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	statementTableScrollTableTdComposition5: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		color: {
			default: colors.inkMuted,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: `${spacing.xs} ${spacing.sm}`,
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	statementTableScrollTableThComposition4: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "left",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		padding: `${spacing.xs} ${spacing.sm}`,
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	tableDataTdNumComposition5: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: `${spacing.xs} ${spacing.sm}`,
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "right",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	statementTableScrollTableComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		width: "100%",
		minWidth: {
			default: null,
			"@media (max-width: 520px)": "640px",
			"@media (max-width: 640px)": "0",
		},
		display: {
			default: null,
			"@media (max-width: 640px)": "block",
		},
		borderCollapse: "collapse",
		fontSize: typography.sizeSm,
	},
	statementTableScrollTableThComposition5: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	statementTableScrollTableThComposition6: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "left",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
	},
	tableStatementTableTdAccountNoComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: colors.inkMuted,
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
		fontFamily: typography.monoFamily,
		whiteSpace: "nowrap",
	},
	statementTableScrollTableTdComposition6: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		color: {
			default: colors.inkMuted,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		whiteSpace: "nowrap",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	statementTableScrollTableTdComposition7: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	tableDataTdNumComposition6: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "right",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	statementTableScrollTableTdComposition8: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		fontWeight: {
			default: 700,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		fontSize: {
			default: typography.sizeMd,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		borderTop: `2px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.border}`,
		paddingTop: spacing.sm,
		paddingBottom: spacing.sm,
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
	},
	tableDataTdNumComposition7: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		fontWeight: {
			default: 700,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		fontSize: {
			default: typography.sizeMd,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		borderTop: `2px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.border}`,
		paddingTop: spacing.sm,
		paddingBottom: spacing.sm,
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "right",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
	},
	statementTableScrollTableTdComposition9: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		color: colors.inkMuted,
		whiteSpace: "nowrap",
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	statementTableScrollTableTdComposition10: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontWeight: 700,
		fontSize: typography.sizeMd,
		borderTop: `2px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.border}`,
		paddingTop: spacing.sm,
		paddingBottom: spacing.sm,
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	statementResultNegativeTdNumComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontWeight: 700,
		fontSize: typography.sizeMd,
		borderTop: `2px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.border}`,
		paddingTop: spacing.sm,
		paddingBottom: spacing.sm,
		textAlign: "right",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		color: colors.danger,
	},
	statementTableScrollTableThComposition7: {
		padding: `${spacing.xs} ${spacing.sm}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	statementTableScrollTableThComposition8: {
		padding: `${spacing.xs} ${spacing.sm}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "left",
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	statementTableScrollTableTdComposition11: {
		padding: `${spacing.xs} ${spacing.sm}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		textAlign: "left",
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	tableDataTdNumComposition8: {
		padding: `${spacing.xs} ${spacing.sm}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "right",
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	statusGridComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		marginBottom: spacing.lg,
		gridTemplateColumns: {
			default: "repeat(2, 1fr)",
			"@media (max-width: 640px)": "1fr",
		},
		display: "grid",
		gap: spacing.md,
	},
	tableDataTdNumComposition9: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontWeight: 700,
		fontSize: typography.sizeMd,
		borderTop: `2px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.border}`,
		paddingTop: spacing.sm,
		paddingBottom: spacing.sm,
		textAlign: "right",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	groupAsOfInputComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
		padding: spacing.xs,
		border: `1px solid ${colors.borderStrong}`,
		borderRadius: rounded.sm,
		background: colors.paperRaised,
		color: colors.ink,
	},
	accountsViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	accountsViewBtnComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	accountsViewInputComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	tableDataComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		width: "100%",
		minWidth: {
			default: null,
			"@media (max-width: 520px)": "0",
			"@media (max-width: 640px)": "0",
		},
		display: {
			default: null,
			"@media (max-width: 640px)": "block",
		},
		borderCollapse: "collapse",
		fontSize: typography.sizeSm,
	},
	tableDataTdComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		color: {
			default: colors.inkMuted,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: `${spacing.xs} ${spacing.sm}`,
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	accrualsViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	pageBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	annualReportViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	filterBarSelectComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
		width: "100%",
	},
	annualReportViewBtnComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	integrityViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	integrityViewBtnComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	bankAccountsViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	bankAccountsViewBtnComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	tableDataTdComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: `${spacing.xs} ${spacing.sm}`,
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	bilagsmailViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	rubrikCopyBtnDisabledComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: {
			default: "pointer",
			":disabled": "not-allowed",
		},
		fontSize: typography.sizeXs,
		fontWeight: 600,
		padding: `2px ${spacing.xs}`,
		borderRadius: rounded.sm,
		border: `1px solid ${colors.border}`,
		background: {
			default: colors.paperRaised,
			":hover:not(:disabled)": `${"var(--surface-hover, var(--surface)"})`,
		},
		color: colors.ink,
		borderColor: {
			default: null,
			":hover:not(:disabled)": colors.ink,
		},
		opacity: {
			default: null,
			":disabled": "0.45",
		},
	},
	rubrikCopyCsvDisabledComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: {
			default: "pointer",
			":disabled": "not-allowed",
		},
		fontSize: typography.sizeXs,
		fontWeight: 600,
		padding: `2px ${spacing.xs}`,
		borderRadius: rounded.sm,
		border: `1px solid ${colors.border}`,
		background: {
			default: colors.paperRaised,
			":hover:not(:disabled)": `${"var(--surface-hover, var(--surface)"})`,
		},
		color: colors.ink,
		borderColor: {
			default: null,
			":hover:not(:disabled)": colors.ink,
		},
		opacity: {
			default: null,
			":disabled": "0.45",
		},
	},
	tableStatementTableComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		width: "100%",
		minWidth: {
			default: null,
			"@media (max-width: 520px)": "0",
			"@media (max-width: 640px)": "0",
		},
		display: {
			default: null,
			"@media (max-width: 640px)": "block",
		},
		borderCollapse: "collapse",
		fontSize: typography.sizeSm,
	},
	tableStatementTableTdAccountNoComposition4: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: colors.inkMuted,
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
		fontFamily: typography.monoFamily,
		whiteSpace: "nowrap",
	},
	tableStatementTableTdComposition5: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		color: {
			default: colors.inkMuted,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		whiteSpace: "nowrap",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	tableStatementTableTdComposition6: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	rowActionsComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: "flex",
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
			"@media print": "none",
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: spacing.xs,
		padding: `${spacing.xs} ${spacing.sm}`,
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "left",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
		alignItems: "center",
		flexWrap: {
			default: null,
			"@media (max-width: 640px)": "wrap",
			"@media (max-width: 639px)": "wrap",
		},
	},
	pageBtnComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	pageSelectComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	attentionViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	modalCheckboxInputComposition3: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
		width: "auto",
		padding: "0",
	},
	statementSectionHeadThComposition: {
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.borderStrong}`,
		color: colors.ink,
		fontWeight: 600,
		fontSize: typography.sizeMd,
		textTransform: "none",
		letterSpacing: "0",
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		background: colors.paper,
		fontFamily: typography.headlineFamily,
		paddingTop: spacing.sm,
		paddingBottom: spacing.sm,
	},
	emptyInlineComposition: {
		textAlign: "left",
		padding: `${spacing.xs} 0`,
		borderBottom: `1px solid ${colors.border}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		color: colors.inkMuted,
		fontSize: typography.sizeSm,
	},
	mutedComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.inkMuted,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
	},
	tdNumComposition2: {
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		color: colors.inkMuted,
	},
	statementSubtotalTdComposition: {
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.borderStrong}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontWeight: 600,
		borderTop: `1px solid ${colors.borderStrong}`,
	},
	statementSubtotalTdComposition2: {
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.borderStrong}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		fontWeight: 600,
		borderTop: `1px solid ${colors.borderStrong}`,
	},
	statementSubtotalTdComposition3: {
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.borderStrong}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		color: colors.inkMuted,
		fontWeight: 600,
		borderTop: `1px solid ${colors.borderStrong}`,
	},
	tableDataTdNumComposition10: {
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		textAlign: "right",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	numComposition: {
		textAlign: "left",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
		borderBottom: `1px solid ${colors.border}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
	},
	dataInputComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		minHeight: "44px",
	},
	tableDataTdNumComposition11: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		padding: `${spacing.xs} ${spacing.sm}`,
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		textAlign: "right",
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
		borderBottom: `1px solid ${colors.border}`,
	},
	periodsViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
	periodsViewBtnComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	periodsViewTableTableComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		width: {
			default: "100%",
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 520px)": "640px",
			"@media (max-width: 640px)": "0",
		},
		display: {
			default: null,
			"@media (max-width: 640px)": "block",
		},
	},
	periodsViewTableTableThComposition: {
		textAlign: "left",
		padding: `${spacing.xs} ${spacing.sm}`,
		borderBottom: `1px solid ${colors.border}`,
		color: colors.inkMuted,
		fontWeight: 600,
		fontSize: typography.sizeXs,
		textTransform: "uppercase",
		letterSpacing: "0.4px",
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
	},
	periodsViewTableTableTdComposition: {
		textAlign: {
			default: "left",
			"@media (max-width: 640px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 640px)": `${spacing.xxs} 0`,
		},
		borderBottom: `1px solid ${colors.border}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		color: {
			default: colors.inkMuted,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		whiteSpace: "nowrap",
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
	},
	periodsViewTableTableTdComposition2: {
		textAlign: {
			default: "left",
			"@media (max-width: 640px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 640px)": `${spacing.xxs} 0`,
		},
		borderBottom: `1px solid ${colors.border}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		color: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
	},
	periodsViewTableTableTdComposition3: {
		textAlign: {
			default: "left",
			"@media (max-width: 640px)": "left",
		},
		padding: {
			default: `${spacing.xs} ${spacing.sm}`,
			"@media (max-width: 640px)": `${spacing.xxs} 0`,
		},
		borderBottom: `1px solid ${colors.border}`,
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		color: {
			default: colors.inkMuted,
			"@media (max-width: 640px)": {
				default: null,
				"::before": colors.inkMuted,
			},
		},
		position: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "sticky",
			},
		},
		right: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": "0",
			},
		},
		background: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": colors.paperRaised,
			},
		},
		boxShadow: {
			default: null,
			"@media (max-width: 520px)": {
				default: null,
				":last-child": `-2px 0 0 ${colors.border}`,
			},
		},
		display: {
			default: null,
			"@media (max-width: 640px)": {
				default: "grid",
				":empty": "none",
			},
		},
		width: {
			default: null,
			"@media (max-width: 640px)": "100%",
		},
		minWidth: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		gridTemplateColumns: {
			default: null,
			"@media (max-width: 640px)": "minmax(7rem, 42%) minmax(0, 1fr)",
		},
		gap: {
			default: null,
			"@media (max-width: 640px)": spacing.xs,
		},
		border: {
			default: null,
			"@media (max-width: 640px)": "0",
		},
		overflowWrap: {
			default: null,
			"@media (max-width: 640px)": "anywhere",
		},
		content: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "attr(data-label)",
			},
		},
		fontSize: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": typography.sizeXs,
			},
		},
		fontWeight: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": 600,
			},
		},
		textTransform: {
			default: null,
			"@media (max-width: 640px)": {
				default: null,
				"::before": "uppercase",
			},
		},
	},
	cvrFactComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		display: "flex",
		justifyContent: "space-between",
		gap: spacing.md,
		padding: `${spacing.xs} 0`,
		borderBottom: {
			default: `1px solid ${colors.border}`,
			":last-child": "none",
		},
	},
	tableStatementTableTdNumMutedComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "1px",
		},
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		color: colors.inkMuted,
		fontWeight: 700,
		fontSize: typography.sizeMd,
		borderTop: `2px solid ${colors.borderStrong}`,
		borderBottom: `1px solid ${colors.border}`,
		paddingTop: spacing.sm,
		paddingBottom: spacing.sm,
		textAlign: "right",
		padding: {
			default: `${spacing.xs} ${spacing.md}`,
			"@media (max-width: 640px)": `${spacing.xs} ${spacing.sm}`,
		},
	},
	exceptionsViewBtnComposition: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		color: colors.accent,
		textDecoration: {
			default: "none",
			":hover": "underline",
		},
		minHeight: "44px",
	},
	exceptionsViewBtnComposition2: {
		boxSizing: "border-box",
		outline: {
			default: null,
			":focus-visible": `2px solid ${colors.accent}`,
		},
		outlineOffset: {
			default: null,
			":focus-visible": "2px",
		},
		font: "inherit",
		cursor: "pointer",
		minHeight: "44px",
	},
});
