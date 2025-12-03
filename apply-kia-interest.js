const api = require("@actual-app/api");
const {
  closeBudget,
  ensurePayee,
  getAccountBalance,
  getTransactions,
  getAccountNote,
  openBudget,
  showPercent,
  applyBankPayment,
  fromCents,
} = require("./utils");
require("dotenv").config();

(async () => {
  const weeklyRate = 0.00133978648017598;

  await openBudget();

  const payeeId = await ensurePayee(
    process.env.INTEREST_PAYEE_NAME || "Loan Interest"
  );

  const accounts = await api.getAccounts();
  for (const account of accounts) {
    if (account.closed) {
      continue;
    }

    const note = await getAccountNote(account);

    // Kia Carnival 2026
    if (note && account.id === "a1d08c47-9e63-4f46-bd36-d4380098844c") {
      if (note.indexOf("interestRate:") > -1) {
        let interestRate = parseFloat(
          note.split("interestRate:")[1].split(" ")[0]
        );

        const transactions = await getTransactions(account);
        const lastTransaction = transactions[0];
        const payment = lastTransaction.amount;
        const interestTransactionDate = new Date(lastTransaction.date);
        const paymentDate = interestTransactionDate.toISOString().split("T")[0];

        const cutoff = new Date(interestTransactionDate);
        cutoff.setDate(cutoff.getDate() - 1);
        const balance = await getAccountBalance(account, cutoff);

        const { interest: compoundedInterest, newBalance } = applyBankPayment(
          balance,
          payment,
          weeklyRate
        );

        interestRate = showPercent(interestRate);

        console.log(`== ${account.name} ==`);
        console.log(` -> Balance:  ${fromCents(balance)}`);
        console.log(`      as of ${cutoff.toISOString().split("T")[0]}`);
        console.log(` -> Payment on:   ${paymentDate}`);
        console.log(
          ` -> Interest: ${fromCents(compoundedInterest)} (${interestRate})`
        );
        console.log(` -> New Balance: ${fromCents(newBalance)}`);

        if (compoundedInterest) {
          await api.importTransactions(account.id, [
            {
              date: paymentDate,
              payee: payeeId,
              amount: compoundedInterest,
              cleared: true,
              notes: `Intérêt pour 1 semaine à ${interestRate}`,
            },
          ]);
        }
      }
    }
  }

  await closeBudget();
})();
