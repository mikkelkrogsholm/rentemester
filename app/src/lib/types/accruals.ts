// #337 — Periodisering / accrual register.

export type AccrualRegisterRow = {
	accrualId: number;
	accrualType: "prepaid_expense" | "accrued_expense" | "deferred_revenue";
	description: string;
	totalAmount: number;
	recognitionPeriods: number;
	recognizedPeriods: number;
	recognizedAmount: number;
	remainingAmount: number;
	fullyRecognized: boolean;
	balanceAccountNo: string;
	resultAccountNo: string;
	firstRecognitionDate: string;
	periodStepMonths: number;
};

export type AccrualRegisterReport = {
	ok: boolean;
	accruals: AccrualRegisterRow[];
	totals: {
		totalAmount: number;
		recognizedAmount: number;
		remainingAmount: number;
	};
	errors: string[];
};

export type CompanyAccrualsResponse = {
	ok: true;
	accruals: {
		nativeAccruals?: boolean;
		slug: string;
		company: {
			name: string;
			cvr: string | null;
			country: string;
			currency: string;
		};
		report: AccrualRegisterReport;
	};
};

export type AccrualRegisterInput = {
	accrualType: AccrualRegisterRow["accrualType"];
	description: string;
	totalAmount: number;
	recognitionPeriods: number;
	firstRecognitionDate: string;
	registrationDate: string;
	periodStepMonths: number;
	balanceAccountNo: string;
	resultAccountNo: string;
	settlementAccountNo: string;
	documentId: number;
};
export type AccrualSchedule = {
	accrualId: number;
	documentId: number;
	accrualType: AccrualRegisterRow["accrualType"];
	schedule: Array<{
		periodIndex: number;
		recognitionDate: string;
		amount: number;
		recognized: boolean;
	}>;
};
