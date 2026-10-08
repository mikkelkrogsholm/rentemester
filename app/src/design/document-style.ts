import mono400 from "@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-400-normal.woff2" with {
	type: "file",
};
import mono500 from "@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-500-normal.woff2" with {
	type: "file",
};
import mono400Ext from "@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-ext-400-normal.woff2" with {
	type: "file",
};
import mono500Ext from "@fontsource/ibm-plex-mono/files/ibm-plex-mono-latin-ext-500-normal.woff2" with {
	type: "file",
};
import sans400 from "@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-400-normal.woff2" with {
	type: "file",
};
import sans500 from "@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-500-normal.woff2" with {
	type: "file",
};
import sans600 from "@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-600-normal.woff2" with {
	type: "file",
};
import sans700 from "@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-700-normal.woff2" with {
	type: "file",
};
import sans400Ext from "@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-ext-400-normal.woff2" with {
	type: "file",
};
import sans500Ext from "@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-ext-500-normal.woff2" with {
	type: "file",
};
import sans600Ext from "@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-ext-600-normal.woff2" with {
	type: "file",
};
import sans700Ext from "@fontsource/ibm-plex-sans/files/ibm-plex-sans-latin-ext-700-normal.woff2" with {
	type: "file",
};
import serif500 from "@fontsource/source-serif-4/files/source-serif-4-latin-500-normal.woff2" with {
	type: "file",
};
import serif600 from "@fontsource/source-serif-4/files/source-serif-4-latin-600-normal.woff2" with {
	type: "file",
};
import serif700 from "@fontsource/source-serif-4/files/source-serif-4-latin-700-normal.woff2" with {
	type: "file",
};
import serif500Ext from "@fontsource/source-serif-4/files/source-serif-4-latin-ext-500-normal.woff2" with {
	type: "file",
};
import serif600Ext from "@fontsource/source-serif-4/files/source-serif-4-latin-ext-600-normal.woff2" with {
	type: "file",
};
import serif700Ext from "@fontsource/source-serif-4/files/source-serif-4-latin-ext-700-normal.woff2" with {
	type: "file",
};
import * as stylex from "@stylexjs/stylex";
import { cockpitStyles } from "./cockpit.stylex";
import { typography } from "./tokens.stylex";

const styles = stylex.create({
	document: {
		margin: 0,
		colorScheme: "light",
		fontFamily: typography.bodyFamily,
		fontSize: typography.bodySize,
		lineHeight: typography.bodyLineHeight,
	},
	scrollLocked: { overflow: "hidden" },
});
let scrollLocks = 0;
/** Each native dialog owns one lock. Closing a nested dialog leaves its parent locked. */
export function lockDocumentScroll() {
	const element = document.documentElement;
	const classes =
		stylex.props(styles.scrollLocked).className?.split(" ").filter(Boolean) ??
		[];
	if (scrollLocks++ === 0) element.classList.add(...classes);
	let released = false;
	return () => {
		if (released) return;
		released = true;
		if (--scrollLocks === 0) element.classList.remove(...classes);
	};
}

const fonts = [
	[
		"IBM Plex Sans",
		"400",
		sans400,
		"U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
	],
	[
		"IBM Plex Sans",
		"400",
		sans400Ext,
		"U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
	],
	[
		"IBM Plex Sans",
		"500",
		sans500,
		"U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
	],
	[
		"IBM Plex Sans",
		"500",
		sans500Ext,
		"U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
	],
	[
		"IBM Plex Sans",
		"600",
		sans600,
		"U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
	],
	[
		"IBM Plex Sans",
		"600",
		sans600Ext,
		"U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
	],
	[
		"IBM Plex Sans",
		"700",
		sans700,
		"U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
	],
	[
		"IBM Plex Sans",
		"700",
		sans700Ext,
		"U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
	],
	[
		"IBM Plex Mono",
		"400",
		mono400,
		"U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
	],
	[
		"IBM Plex Mono",
		"400",
		mono400Ext,
		"U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
	],
	[
		"IBM Plex Mono",
		"500",
		mono500,
		"U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
	],
	[
		"IBM Plex Mono",
		"500",
		mono500Ext,
		"U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
	],
	[
		"Source Serif 4",
		"500",
		serif500,
		"U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
	],
	[
		"Source Serif 4",
		"500",
		serif500Ext,
		"U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
	],
	[
		"Source Serif 4",
		"600",
		serif600,
		"U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
	],
	[
		"Source Serif 4",
		"600",
		serif600Ext,
		"U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
	],
	[
		"Source Serif 4",
		"700",
		serif700,
		"U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
	],
	[
		"Source Serif 4",
		"700",
		serif700Ext,
		"U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
	],
] as const;
let initialized = false;
/** Font assets are local and registered before the first React render. */
export function initializeDocumentStyle() {
	if (initialized) return;
	initialized = true;
	document.documentElement.classList.add(
		...(stylex
			.props(cockpitStyles.element, cockpitStyles.html, styles.document)
			.className?.split(" ") ?? []),
	);
	document.body.classList.add(
		...(stylex
			.props(cockpitStyles.element, cockpitStyles.body)
			.className?.split(" ") ?? []),
	);
	const root = document.getElementById("root");
	if (root)
		root.classList.add(
			...(stylex.props(cockpitStyles.element).className?.split(" ") ?? []),
		);
	if (typeof FontFace === "undefined") return;
	for (const [family, weight, url, unicodeRange] of fonts) {
		const face = new FontFace(family, `url(${JSON.stringify(url)})`, {
			weight,
			style: "normal",
			display: "swap",
			unicodeRange,
		});
		document.fonts.add(face);
		// A failed font retains the declared system fallback and remains observable in FontFaceSet.
		void face.load().catch(() => undefined);
	}
}
