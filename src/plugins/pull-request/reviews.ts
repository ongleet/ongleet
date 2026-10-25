export type Review = {
  id: string;
  author: { login: string } | null;
  state: string;
  submittedAt: string | null;
};
export type ReviewCursor = { through: string; ids: string[] };

export function otherReviews(reviews: Review[], viewer: string): Review[] {
  return reviews.filter(
    (review) =>
      review.author?.login &&
      review.author.login.toLowerCase() !== viewer.toLowerCase() &&
      review.state !== 'PENDING' &&
      review.submittedAt &&
      Number.isFinite(Date.parse(review.submittedAt)),
  );
}
export function unreadReviews(reviews: Review[], cursor: ReviewCursor): Review[] {
  return reviews.filter(
    (review) =>
      review.submittedAt! > cursor.through ||
      (review.submittedAt === cursor.through && !cursor.ids.includes(review.id)),
  );
}
export function acknowledgeReviews(
  reviews: Review[],
  previous: ReviewCursor = { through: '', ids: [] },
): ReviewCursor {
  const through = reviews.reduce(
    (latest, review) => (review.submittedAt! > latest ? review.submittedAt! : latest),
    previous.through,
  );
  return {
    through,
    ids: [
      ...new Set([
        ...(through === previous.through ? previous.ids : []),
        ...reviews.filter((review) => review.submittedAt === through).map((review) => review.id),
      ]),
    ],
  };
}
