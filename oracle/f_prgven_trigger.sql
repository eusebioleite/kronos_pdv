create or replace trigger kronos.kronos_pdv_prgven after
	insert or update or delete on kronos.f_prgven
	for each row
declare
	v_tipped char(1) := 'F';
	v_status char(1);
begin
	select pdv_tipped,pdv_status
	  into
		v_tipped,v_status
	  from f_pedvenda
	 where pdv_numped = :new.prv_numped;

	if
		deleting
		and v_tipped = 'A'
		and v_status = 'A'
	then
		update inventario.cards
		   set
			status = 'EXCLUIR'
		 where tipo = 'ABERTO'
		   and pedido = :old.prv_numped
		   and indice = :old.prv_indice;

		return;
	end if;

	if
		( inserting
		or updating )
		and v_tipped = 'A'
		and v_status = 'A'
	then
		begin
			insert into inventario.cards (
				tipo,pedido,indice,status
			) values ( 'ABERTO',:new.prv_numped,:new.prv_indice,'NOVO' );
		exception
			when dup_val_on_index then
				null;
		end;
	end if;

exception
	when others then
		null;
end;