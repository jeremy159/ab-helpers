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
  const monthlyRate = 0.003543453216552734375;

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

    //  Maison Proulx
    if (note && account.id === "eda51ae0-7510-4382-b6d7-2748ccb7f219") {
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
        cutoff.setDate(cutoff.getMonth() - 1);
        cutoff.setDate(cutoff.getDate() - 1);
        const balance = await getAccountBalance(account, cutoff);

        const { interest: compoundedInterest, newBalance } = applyBankPayment(
          balance,
          payment,
          monthlyRate
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
              notes: `Intérêt pour 1 mois à ${interestRate}`,
            },
          ]);
        }
      }
    }
  }

  await closeBudget();
})();
