import * as stylex from "@stylexjs/stylex";
import { cockpitStyles } from "../design/cockpit.stylex";
// A render helper that wraps a component in a MemoryRouter so route-aware
// components (links, useNavigate, useParams) work under test.

import { render } from "@testing-library/react";
import type { ReactElement } from "react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import {
	COMPANY_ROUTE_REGISTRY,
	COMPANY_TASK_AREAS,
} from "../company-route-registry";
import { CompanyNavigationShell } from "../components/CompanyNav";

const COMPANY_NAVIGATION = {
	routes: COMPANY_ROUTE_REGISTRY,
	areas: COMPANY_TASK_AREAS,
};

export function renderAt(
	ui: ReactElement,
	{ route = "/", path = "*" }: { route?: string; path?: string } = {},
) {
	return render(
		<MemoryRouter initialEntries={[route]}>
			<CompanyNavigationShell navigation={COMPANY_NAVIGATION}>
				<Routes>
					<Route path={path} element={ui} />
					<Route
						path="*"
						element={
							<div
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							/>
						}
					/>
				</Routes>
			</CompanyNavigationShell>
		</MemoryRouter>,
	);
}
